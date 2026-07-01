// dir base on note.html

const electron = require('electron')

var Evt = require('./src/evt');
var app = require('@electron/remote').app; // .require('app');
var basePath = app.getPath('appData') + '/leanote'; // /Users/life/Library/Application Support/Leanote'; // require('nw.gui').App.dataPath;
Evt.setDataBasePath(basePath);
var protocol = require('electron').protocol; // .require('protocol');
// 数据库初始化
var db = require('./src/db');
// db.init();
db.initGlobal();
// 所有service, 与数据库打交道
var Service = {
	notebookService: require('./src/notebook'),
	noteService: require('./src/note'),
	userService: require('./src/user'),
	tagService: require('./src/tag'),
	apiService: require('./src/api'),
	syncServie: require('./src/sync')
};
// 全局变量
var ApiService = Service.apiService;
var UserService = Service.userService;
var SyncService = Service.syncServie;
var NoteService = Service.noteService;
var NotebookService = Service.notebookService;
var TagService = Service.tagService;
var WebService = require('./src/web');
var FileService = require('./src/file');
var EvtService = Evt;
const CommonService = require('./src/common');
const Common = CommonService

// NodeJs
const NodeFs = require('fs');
const NodePath = require('path');
const Resanitize = require('./src/resanitize');

// 分发服务
// route = /note/notebook
// 过时
Service.dispatch = function() {};
var gui = require('./src/gui');

var projectPath = __dirname;

// 确保 leanote CLI 命令已安装
(function ensureLeanoteCLI() {
    var home = require('os').homedir();
    var appRoot = require('@electron/remote').app.getAppPath();

    // 候选 bin 目录，按优先级选第一个可写的
    var candidates = [
        NodePath.join(home, 'bin'),
        NodePath.join(home, '.local', 'bin'),
        NodePath.join(require('@electron/remote').app.getPath('userData'), 'bin')
    ];

    var binDir = null;
    for (var i = 0; i < candidates.length; i++) {
        if (!NodeFs.existsSync(candidates[i])) {
            try { NodeFs.mkdirSync(candidates[i], { recursive: true }); } catch (e) { continue; }
        }
        try {
            NodeFs.accessSync(candidates[i], NodeFs.constants.W_OK);
            binDir = candidates[i];
            break;
        } catch (e) {}
    }

    if (!binDir) {
        console.log('[leanote] No writable bin directory found');
        return;
    }

    // 清理旧版本可能写到 /usr/local/bin 的残留文件
    try { if (NodeFs.existsSync('/usr/local/bin/leanote')) NodeFs.unlinkSync('/usr/local/bin/leanote'); } catch (e) {}

    var wrapperPath = NodePath.join(binDir, 'leanote');
    var versionPath = NodePath.join(binDir, 'leanote.version');

    // 读当前版本号
    var currentVersion;
    try {
        var pkg = JSON.parse(NodeFs.readFileSync(NodePath.join(appRoot, 'package.json'), 'utf-8'));
        currentVersion = pkg.version;
    } catch (e) {
        currentVersion = '2.1.4';
    }

    // 如果 wrapper、脚本、版本文件都完整且版本匹配，跳过安装
    try {
        if (NodeFs.existsSync(wrapperPath) &&
            NodeFs.existsSync(NodePath.join(binDir, 'leanote.js')) &&
            NodeFs.existsSync(versionPath)) {
            var installedVersion = NodeFs.readFileSync(versionPath, 'utf-8').trim();
            if (installedVersion === currentVersion) return;
        }
    } catch (e) {}

    // 查找 leanote.js 源文件（asar 内路径 + 开发模式备选路径）
    var scriptContent = null;
    var tryPaths = [
        NodePath.join(appRoot, 'src', 'leanote_cli.js'),
        NodePath.join(__dirname, '..', 'leanote_cli.js')
    ];
    for (var j = 0; j < tryPaths.length; j++) {
        try {
            scriptContent = NodeFs.readFileSync(tryPaths[j], 'utf-8');
            console.log('[leanote] Found at: ' + tryPaths[j]);
            break;
        } catch (e) {
            console.log('[leanote] Not found at: ' + tryPaths[j]);
        }
    }

    if (!scriptContent) {
        console.log('[leanote] leanote.js not found in any path, skipping install');
        return;
    }

    try {
        var scriptPath = NodePath.join(binDir, 'leanote.js');
        NodeFs.writeFileSync(scriptPath, scriptContent);
        NodeFs.chmodSync(scriptPath, '755');

        var wrapper = '#!/bin/sh\n' +
            'NODE="node"\n' +
            'command -v node >/dev/null 2>&1 || NODE="' +
                process.execPath.replace(/"/g, '\\"') + '"\n' +
            'exec "$NODE" "' + scriptPath.replace(/"/g, '\\"') + '" "$@"\n';
        NodeFs.writeFileSync(wrapperPath, wrapper);
        NodeFs.chmodSync(wrapperPath, '755');

        NodeFs.writeFileSync(versionPath, currentVersion);
        console.log('[leanote] CLI installed to ' + wrapperPath);
    } catch (err) {
        console.log('[leanote] Install failed:', err.message);
    }

    // 写入 shell rc 文件
    var shellRc = NodePath.join(home, '.zshrc');
    if (!NodeFs.existsSync(shellRc)) {
        shellRc = NodePath.join(home, '.bashrc');
        if (!NodeFs.existsSync(shellRc)) {
            shellRc = NodePath.join(home, '.bash_profile');
        }
    }
    var pathLine = 'export PATH="$PATH:' + binDir + '"';
    try {
        if (NodeFs.existsSync(shellRc)) {
            var rcContent = NodeFs.readFileSync(shellRc, 'utf-8');
            if (rcContent.indexOf(binDir) === -1) {
                NodeFs.appendFileSync(shellRc, '\n' + pathLine + '  # leanote\n');
                console.log('[leanote] Added PATH to ' + shellRc);
            }
        }
    } catch (e) {}
})();

// 启动本地 HTTP 服务，供 CLI 通过 API 调用（避免直接写文件不同步）
(function startLocalServer() {
    try {
        require('./src/local_server').start(Service);
    } catch (e) {
        console.log('[leanote] Failed to start local server:', e.message);
    }
})();
