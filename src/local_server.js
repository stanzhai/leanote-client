// 本地 HTTP 服务（运行在渲染进程，随 service.js 启动）。
// 单端口 8912 提供两组相互独立的路由：
//   /api/*  供 leanote CLI 调用（走 app 内 Service，实时同步到运行中的 app）
//   /mcp    MCP Streamable HTTP，供智能体管理笔记（走 leanote_data 文件 I/O，与 CLI 文件模式一致）

var http = require('http');
var crypto = require('crypto');
var dataApi = require('./leanote_data');

var HOST = '127.0.0.1';
var DEFAULT_PORT = 8912;

var server = null;

// ===== MCP session 状态（SDK 懒加载，/mcp 首次访问才加载）=====
var transports = {}; // sessionId -> transport
var transportLastUsed = {}; // sessionId -> timestamp，用于 TTL 回收
var TRANSPORT_MAX = 50; // 最大并发会话数
var TRANSPORT_TTL = 30 * 60 * 1000; // 30 分钟无活动自动清理
var mcpSdk = null;

function loadMcpSdk() {
    if (mcpSdk) return mcpSdk;
    try {
        mcpSdk = {
            McpServer: require('@modelcontextprotocol/sdk/server/mcp.js').McpServer,
            StreamableHTTPServerTransport: require('@modelcontextprotocol/sdk/server/streamableHttp.js')
                .StreamableHTTPServerTransport,
            SSEServerTransport: require('@modelcontextprotocol/sdk/server/sse.js').SSEServerTransport,
            isInitializeRequest: require('@modelcontextprotocol/sdk/types.js').isInitializeRequest,
            z: require('zod'),
        };
    } catch (e) {
        mcpSdk = { error: e };
    }
    return mcpSdk;
}

function start(services, opts) {
    if (server) return;
    opts = opts || {};
    var mcpOnly = !!opts.mcpOnly; // 测试用：只挂 /mcp，不需 services
    var port = opts.port || DEFAULT_PORT;

    server = http.createServer(function (req, res) {
        res.setHeader('Access-Control-Allow-Origin', '*');
        res.setHeader('Access-Control-Allow-Methods', 'GET, POST, DELETE, OPTIONS');
        res.setHeader(
            'Access-Control-Allow-Headers',
            'Content-Type, Accept, Mcp-Session-Id, Last-Event-ID',
        );

        if (req.method === 'OPTIONS') {
            res.writeHead(200);
            res.end();
            return;
        }

        var MAX_BODY = 5 * 1024 * 1024; // 5MB 上限
        var bodyChunks = [];
        var bodyLength = 0;
        var body = '';
        req.on('data', function (chunk) {
            bodyLength += Buffer.byteLength(chunk);
            if (bodyLength > MAX_BODY) {
                req.destroy();
                res.writeHead(413, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify({ error: 'Request body too large' }));
                return;
            }
            bodyChunks.push(chunk);
        });
        req.on('end', function () {
            body = Buffer.concat(bodyChunks).toString();
            logMcpRequest(req, body); // 诊断日志（记录所有请求，排查 HARPA 等外部客户端）
            // MCP Streamable HTTP（新版）
            if (req.url === '/mcp') {
                handleMcp(req, res, body);
                return;
            }
            // /sse：GET 建立旧版 SSE 长连接；POST 当作无状态 JSON-RPC（HARPA 等扩展直接 POST 到该 URL）
            if (req.url === '/sse' || req.url.indexOf('/sse?') === 0) {
                if (req.method === 'GET') {
                    handleSseConnect(req, res);
                } else {
                    handleMcp(req, res, body);
                }
                return;
            }
            if (req.method === 'POST' && (req.url === '/messages' || req.url.indexOf('/messages?') === 0)) {
                handleSseMessage(req, res, body);
                return;
            }
            if (mcpOnly) {
                sendJSON(res, 404, { error: 'Not found' });
                return;
            }
            handleApi(req, res, body, services);
        });
    });

    server.listen(port, HOST, function () {
        console.log('[leanote] Local server on ' + HOST + ':' + port + ' (/api, /mcp)');
    });

    server.on('error', function (err) {
        if (err.code === 'EADDRINUSE') {
            console.log('[leanote] Port ' + port + ' in use, CLI will use file I/O');
        } else {
            console.log('[leanote] Local server error:', err.message);
        }
        server = null;
    });

    // 每 5 分钟清理超过 TTL 未活动的 MCP 会话
    var reapInterval = setInterval(function () {
        var now = Date.now();
        var keys = Object.keys(transportLastUsed);
        for (var i = 0; i < keys.length; i++) {
            var sid = keys[i];
            if (now - transportLastUsed[sid] > TRANSPORT_TTL) {
                delete transportLastUsed[sid];
                if (transports[sid]) {
                    try { transports[sid].close(); } catch (e) {}
                    delete transports[sid];
                }
            }
        }
    }, 5 * 60 * 1000);
    // 允许进程退出（不阻止事件循环）
    if (reapInterval.unref) reapInterval.unref();
}

// =========================================================================
// /api/* —— CLI 调用（用 app 内 Service）
// =========================================================================

function handleApi(req, res, rawBody, services) {
    var NoteService = services.noteService;
    var NotebookService = services.notebookService;
    var TagService = services.tagService;
    var UserService = services.userService;

    var userId = UserService.getCurActiveUserId();
    if (!userId) {
        sendJSON(res, 503, { success: false, error: 'Not logged in' });
        return;
    }

    try {
        if (req.method === 'POST' && req.url === '/api/note/addNote') {
            handleAddNote(res, JSON.parse(rawBody), userId, NoteService, NotebookService, TagService);
        } else if (req.method === 'POST' && req.url === '/api/note/updateNote') {
            handleUpdateNote(res, JSON.parse(rawBody), userId, NoteService, NotebookService, TagService);
        } else if (req.method === 'GET' && req.url === '/api/ping') {
            sendJSON(res, 200, { success: true });
        } else {
            sendJSON(res, 404, { success: false, error: 'Unknown endpoint' });
        }
    } catch (e) {
        sendJSON(res, 400, { success: false, error: e.message });
    }
}

function handleAddNote(res, data, userId, NoteService, NotebookService, TagService) {
    if (!data.title) { sendJSON(res, 400, { success: false, error: 'title is required' }); return; }
    if (!data.content) { sendJSON(res, 400, { success: false, error: 'content is required' }); return; }

    var targetNotebook = data.notebook || 'AI札';
    var tags = data.tags || [];

    NotebookService.getNotebooks(function (notebooks) {
        var notebook = null;
        for (var i = 0; i < notebooks.length; i++) {
            if (notebooks[i].Title === targetNotebook) { notebook = notebooks[i]; break; }
        }

        function createNote(notebookId) {
            var desc = data.content.replace(/\n/g, ' ').replace(/#/g, '').substring(0, 50).trim();
            var note = {
                NoteId: require('./objectid')(),
                UserId: userId,
                NotebookId: notebookId,
                Title: data.title,
                Content: data.content,
                Desc: desc,
                ImgSrc: '',
                Tags: tags,
                Abstract: data.content.substring(0, 500),
                IsMarkdown: true,
                IsNew: true // signals updateNoteOrContent this is a new note
            };

            NoteService.updateNoteOrContent(note, function (inserted) {
                if (!inserted) {
                    sendJSON(res, 500, { success: false, error: 'Failed to create note' });
                    return;
                }
                // Upsert tags
                tagUpsert(tags, TagService);
                sendJSON(res, 200, { success: true, noteId: inserted.NoteId });
            });
        }

        if (notebook) {
            createNote(notebook.NotebookId);
        } else {
            var newNotebookId = require('./objectid')();
            NotebookService.addNotebook(newNotebookId, targetNotebook, '', function (nb) {
                if (!nb) { sendJSON(res, 500, { success: false, error: 'Failed to create notebook' }); return; }
                createNote(newNotebookId);
            });
        }
    });
}

function handleUpdateNote(res, data, userId, NoteService, NotebookService, TagService) {
    if (!data.noteId) { sendJSON(res, 400, { success: false, error: 'noteId is required' }); return; }

    findNoteByPrefix(data.noteId, NoteService, function (note) {
        if (!note) { sendJSON(res, 404, { success: false, error: 'Note not found' }); return; }

        if (data.title !== undefined) note.Title = data.title;
        if (data.content !== undefined) {
            note.Content = data.content;
            note.Desc = data.content.replace(/\n/g, ' ').replace(/#/g, '').substring(0, 50).trim();
            note.Abstract = data.content.substring(0, 500);
        }
        if (data.tags !== undefined) note.Tags = data.tags;

        function saveNote() {
            if (data.tags) tagUpsert(data.tags, TagService);
            NoteService.updateNoteOrContent(note, function (updated) {
                if (!updated) { sendJSON(res, 500, { success: false, error: 'Failed to update' }); return; }
                sendJSON(res, 200, { success: true, noteId: updated.NoteId });
            });
        }

        if (data.notebook !== undefined) {
            NotebookService.getNotebooks(function (notebooks) {
                for (var i = 0; i < notebooks.length; i++) {
                    if (notebooks[i].Title === data.notebook) { note.NotebookId = notebooks[i].NotebookId; break; }
                }
                saveNote();
            });
        } else {
            saveNote();
        }
    });
}

function findNoteByPrefix(noteId, NoteService, callback) {
    NoteService.getNote(noteId, function (note) {
        if (note) { callback(note); return; }
        // Try prefix match
        var db = require('./db');
        db.notes.find({NoteId: new RegExp('^' + noteId.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))}).exec(function (err, notes) {
            if (err || !notes || !notes.length) { callback(null); return; }
            // Return first non-trash, non-local-deleted match
            for (var i = 0; i < notes.length; i++) {
                if (notes[i].IsTrash !== true && notes[i].LocalIsDelete !== true) {
                    callback(notes[i]);
                    return;
                }
            }
            callback(null);
        });
    });
}

function tagUpsert(tags, TagService) {
    if (!tags || !tags.length) return;
    TagService.getTags(function (existing) {
        tags.forEach(function (title) {
            var found = existing.some(function (t) { return t.Tag === title; });
            if (!found) TagService.addOrUpdateTag(title, function () {});
        });
    });
}

// =========================================================================
// /mcp —— MCP Streamable HTTP（用 leanote_data 文件 I/O，与 CLI 文件模式一致）
// =========================================================================

// MCP 诊断日志：默认关闭，设置 LEANOTE_MCP_DEBUG=1 开启。异步写入 + 10MB 轮转。
function logMcpRequest(req, rawBody) {
    if (!process.env.LEANOTE_MCP_DEBUG) return;
    try {
        var fs = require('fs');
        var os = require('os');
        var logPath = os.homedir() + '/leanote_mcp.log';
        var MAX_LOG = 10 * 1024 * 1024; // 10MB

        // 轮转检查（在写入前 stat 当前日志大小）
        try {
            var stat = fs.statSync(logPath);
            if (stat.size > MAX_LOG) {
                // 轮转: .log -> .log.1, .log.1 -> .log.2, 删除 .log.2
                try { fs.unlinkSync(logPath + '.2'); } catch (e) {}
                try { fs.renameSync(logPath + '.1', logPath + '.2'); } catch (e) {}
                try { fs.renameSync(logPath, logPath + '.1'); } catch (e) {}
            }
        } catch (e) {}

        var entry = '[MCP] ' + new Date().toISOString() + '\n' +
            '  ' + req.method + ' ' + req.url + '\n' +
            '  mcp-session-id: ' + (req.headers['mcp-session-id'] || '(none)') + '\n' +
            '  accept: ' + (req.headers['accept'] || '(none)') + '\n' +
            '  content-type: ' + (req.headers['content-type'] || '(none)') + '\n' +
            '  origin: ' + (req.headers['origin'] || '(none)') + '\n' +
            '  body: ' + (rawBody || '').slice(0, 800) + '\n\n';
        // 异步写入，避免阻塞事件循环
        fs.appendFile(logPath, entry, function () {});
    } catch (e) {}
}

function handleMcp(req, res, rawBody) {
    var sdk = loadMcpSdk();
    if (sdk.error) {
        sendJSON(res, 500, {
            jsonrpc: '2.0',
            error: { code: -32603, message: 'MCP SDK unavailable: ' + sdk.error.message },
            id: null,
        });
        return;
    }

    var parsedBody = null;
    if (rawBody) {
        try {
            parsedBody = JSON.parse(rawBody);
        } catch (e) {
            sendJSON(res, 400, {
                jsonrpc: '2.0',
                error: { code: -32700, message: 'Parse error' },
                id: null,
            });
            return;
        }
    }

    var sessionId = req.headers['mcp-session-id'];
    var method = req.method;

    if (method === 'POST') {
        var transport;
        // 已有 session：复用 transport，更新最后活动时间
        if (sessionId && transports[sessionId]) {
            transport = transports[sessionId];
            transportLastUsed[sessionId] = Date.now();
            transport.handleRequest(req, res, parsedBody).catch(function (e) { mcpFail(res, e); });
            return;
        }
        // 新的初始化请求
        if (!sessionId && parsedBody && sdk.isInitializeRequest(parsedBody)) {
            // 会话数上限保护
            if (Object.keys(transports).length >= TRANSPORT_MAX) {
                sendJSON(res, 503, {
                    jsonrpc: '2.0',
                    error: { code: -32000, message: 'Too many concurrent sessions' },
                    id: null,
                });
                return;
            }
            transport = new sdk.StreamableHTTPServerTransport({
                sessionIdGenerator: function () { return crypto.randomUUID(); },
                onsessioninitialized: function (sid) {
                    transports[sid] = transport;
                    transportLastUsed[sid] = Date.now();
                },
                enableDnsRebindingProtection: false,
            });
            transport.onclose = function () {
                if (transport.sessionId) {
                    delete transports[transport.sessionId];
                    delete transportLastUsed[transport.sessionId];
                }
            };
            var mcpServer = createMcpServer(sdk);
            mcpServer.connect(transport).then(function () {
                return transport.handleRequest(req, res, parsedBody);
            }).catch(function (e) { mcpFail(res, e); });
            return;
        }
        // 无 session 且非 initialize：无状态处理
        // 兼容 HARPA 等直接 POST JSON-RPC 的客户端（不做 initialize 握手、不维护 session）
        var statelessTransport = new sdk.StreamableHTTPServerTransport({ sessionIdGenerator: undefined });
        var statelessServer = createMcpServer(sdk);
        statelessTransport.onclose = function () {
            // 无状态 transport 用完后清理
        };
        statelessServer.connect(statelessTransport).then(function () {
            return statelessTransport.handleRequest(req, res, parsedBody);
        }).catch(function (e) { mcpFail(res, e); });
        return;
    }

    if (method === 'GET' || method === 'DELETE') {
        if (!sessionId || !transports[sessionId]) {
            sendJSON(res, 400, { error: 'Invalid or missing session ID' });
            return;
        }
        transportLastUsed[sessionId] = Date.now();
        transports[sessionId].handleRequest(req, res).catch(function (e) { mcpFail(res, e); });
        return;
    }

    sendJSON(res, 405, { error: 'Method not allowed' });
}

function createMcpServer(sdk) {
    var mcp = new sdk.McpServer({ name: 'leanote', version: '1.0.0' });
    registerTools(mcp, sdk.z);
    return mcp;
}

// =========================================================================
// /sse + /messages —— 旧版 HTTP+SSE 传输（兼容浏览器扩展等老客户端）
// 流程：GET /sse 建立 SSE 长连接 → POST /messages 收发 JSON-RPC 消息
// =========================================================================

var sseTransport = null; // 单 SSE 连接（与 MCP 官方示例一致）

function handleSseConnect(req, res) {
    var sdk = loadMcpSdk();
    if (sdk.error) { sendJSON(res, 500, { error: 'MCP SDK unavailable' }); return; }
    try {
        sseTransport = new sdk.SSEServerTransport('/messages', res);
        var sseServer = createMcpServer(sdk);
        sseServer.connect(sseTransport).catch(function (e) {
            console.log('[leanote] SSE connect error:', e.message);
        });
    } catch (e) {
        console.log('[leanote] SSE init error:', e.message);
    }
}

function handleSseMessage(req, res, rawBody) {
    if (!sseTransport) {
        sendJSON(res, 400, { error: 'No SSE connection (GET /sse first)' });
        return;
    }
    sseTransport.handlePostMessage(req, res, rawBody).catch(function (e) {
        console.log('[leanote] SSE post error:', e.message);
    });
}

// --- MCP 工具结果辅助 ---

function ok(obj) {
    return { content: [{ type: 'text', text: JSON.stringify(obj, null, 2) }] };
}
function fail2(message) {
    return { isError: true, content: [{ type: 'text', text: String(message) }] };
}
// 包装 handler：捕获异常返回结构化错误，避免单个工具失败拖垮会话
function safe(handler) {
    return async function (args) {
        try {
            return ok(await handler(args));
        } catch (e) {
            return fail2(e.message);
        }
    };
}

function registerTools(server, z) {
    // 只读：笔记本 / 标签
    server.registerTool(
        'list_notebooks',
        { description: '列出所有笔记本及其笔记数量。(List all notebooks with note counts.)' },
        safe(function () { return dataApi.listNotebooks(); }),
    );
    server.registerTool(
        'list_tags',
        { description: '列出所有标签。(List all tags.)' },
        safe(function () { return dataApi.listTags(); }),
    );

    // 写：新建笔记
    server.registerTool(
        'add_note',
        {
            description:
                '新建一条 Markdown 笔记。(Create a new markdown note.) notebook 默认 "AI札"，不存在则自动创建。',
            inputSchema: {
                title: z.string().describe('笔记标题 (note title, required)'),
                content: z.string().describe('Markdown 正文 (markdown content, required)'),
                tags: z.array(z.string()).optional().describe('标签数组 (tags array)'),
                notebook: z
                    .string()
                    .optional()
                    .describe('目标笔记本名 (target notebook title; default "AI札")'),
            },
        },
        safe(function (args) {
            return dataApi.createNote({
                title: args.title,
                content: args.content,
                tags: args.tags || [],
                notebook: args.notebook,
            });
        }),
    );

    // 读：列表
    server.registerTool(
        'list_notes',
        {
            description:
                '列出最近的笔记。(List recent notes.) 可按笔记本名 / 标签过滤，按更新时间倒序。',
            inputSchema: {
                notebook: z.string().optional().describe('按笔记本名过滤 (filter by notebook title)'),
                tag: z.string().optional().describe('按标签过滤 (filter by tag)'),
                limit: z.number().optional().describe('最大条数 (max results, default 20)'),
            },
        },
        safe(function (args) {
            return dataApi.listNotes({ notebook: args.notebook, tag: args.tag, limit: args.limit });
        }),
    );

    // 读：详情
    server.registerTool(
        'get_note',
        {
            description: '按 ID 查看笔记完整内容，支持 ID 前缀。(Get full note content by id or prefix.)',
            inputSchema: { noteId: z.string().describe('笔记 ID 或前缀 (note id or prefix)') },
        },
        safe(function (args) { return dataApi.getNote(args.noteId); }),
    );

    // 读：搜索
    server.registerTool(
        'search_notes',
        {
            description:
                '按关键字搜索笔记（标题 / 正文 / 标签），按相关度排序。(Search notes by keyword.)',
            inputSchema: {
                keyword: z.string().describe('关键字 (keyword)'),
                limit: z.number().optional().describe('最大条数 (max results, default 20)'),
            },
        },
        safe(function (args) { return dataApi.searchNotes(args.keyword, args.limit); }),
    );

    // 写：更新
    server.registerTool(
        'update_note',
        {
            description:
                '更新现有笔记（按 ID 或前缀）。(Update an existing note by id/prefix.) 至少提供一个可更新字段；tags 为替换语义。',
            inputSchema: {
                noteId: z.string().describe('笔记 ID 或前缀 (note id or prefix)'),
                title: z.string().optional().describe('新标题 (new title)'),
                content: z.string().optional().describe('新 Markdown 正文 (new markdown content)'),
                tags: z
                    .array(z.string())
                    .optional()
                    .describe('新标签数组（替换现有） (new tags array, replaces existing)'),
                notebook: z.string().optional().describe('移动到该笔记本 (move to this notebook by title)'),
            },
        },
        safe(function (args) {
            return dataApi.updateNoteFields({
                noteId: args.noteId,
                title: args.title,
                content: args.content,
                tags: args.tags,
                notebook: args.notebook,
            });
        }),
    );

    // 写：删除（移到回收站）
    server.registerTool(
        'delete_note',
        {
            description: '删除笔记（移到回收站），按 ID 或前缀。(Move a note to trash by id/prefix.)',
            inputSchema: { noteId: z.string().describe('笔记 ID 或前缀 (note id or prefix)') },
        },
        safe(function (args) { return dataApi.deleteNote(args.noteId); }),
    );
}

// =========================================================================
// 共用
// =========================================================================

function sendJSON(res, statusCode, data) {
    try {
        res.writeHead(statusCode, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify(data));
    } catch (e) {}
}

function mcpFail(res, e) {
    console.log('[leanote] MCP request error:', e && e.message);
    sendJSON(res, 500, {
        jsonrpc: '2.0',
        error: { code: -32603, message: (e && e.message) || 'Internal error' },
        id: null,
    });
}

module.exports = { start: start };
