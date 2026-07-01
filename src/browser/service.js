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
    var source = NodePath.join(__dirname, '..', '..', 'bin', 'leanote.js');

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

    if (!binDir) return;

    var target = NodePath.join(binDir, 'leanote');
    if (NodeFs.existsSync(target)) return;

    try {
        var wrapper = '#!/bin/sh\n' +
            'NODE="node"\n' +
            'command -v node >/dev/null 2>&1 || NODE="' +
                process.execPath.replace(/"/g, '\\"') + '"\n' +
            'exec "$NODE" "' + source.replace(/"/g, '\\"') + '" "$@"\n';
        NodeFs.writeFileSync(target, wrapper);
        NodeFs.chmodSync(target, '755');
        console.log('[leanote] CLI installed to ' + target);
    } catch (err) {
        console.log('[leanote] Auto-install failed:', err.message);
    }

    // 将 bin 目录写入 shell rc 文件，确保终端可用
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
            if (rcContent.indexOf(pathLine) === -1) {
                NodeFs.appendFileSync(shellRc, '\n' + pathLine + '  # leanote\n');
                console.log('[leanote] Added PATH to ' + shellRc);
            }
        }
    } catch (e) {}
})();
