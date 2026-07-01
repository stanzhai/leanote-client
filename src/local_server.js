// Local HTTP API server that runs in the renderer process.
// Allows the leanote CLI to communicate with the running app's NeDB instance.
var http = require('http');

var server = null;

function start(services) {
    if (server) return;

    var host = '127.0.0.1';
    var port = 8912;

    try {
        server = http.createServer(function (req, res) {
            res.setHeader('Access-Control-Allow-Origin', '*');
            res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
            res.setHeader('Access-Control-Allow-Headers', 'Content-Type');

            if (req.method === 'OPTIONS') {
                res.writeHead(200);
                res.end();
                return;
            }

            var body = '';
            req.on('data', function (chunk) { body += chunk; });
            req.on('end', function () {
                handleRequest(req, res, body, services);
            });
        });

        server.listen(port, host, function () {
            console.log('[leanote] Local API server on ' + host + ':' + port);
        });

        server.on('error', function (err) {
            if (err.code === 'EADDRINUSE') {
                console.log('[leanote] Port ' + port + ' in use, CLI will use file I/O');
            }
            server = null;
        });
    } catch (e) {
        console.log('[leanote] Failed to start local API server:', e.message);
        server = null;
    }
}

function handleRequest(req, res, rawBody, services) {
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

function sendJSON(res, statusCode, data) {
    res.writeHead(statusCode, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(data));
}

module.exports = { start: start };
