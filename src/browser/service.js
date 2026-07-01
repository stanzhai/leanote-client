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
    var binDir = '/usr/local/bin';
    var target = NodePath.join(binDir, 'leanote');
    var source = NodePath.join(__dirname, '..', '..', 'bin', 'leanote.js');

    // 已存在则跳过
    if (NodeFs.existsSync(target)) return;

    // 检查 /usr/local/bin 是否可写
    try { NodeFs.accessSync(binDir, NodeFs.constants.W_OK); } catch (e) { return; }

    try {
        // 用 Electron 自带的 Node 作为后备，不依赖用户系统是否装了 node
        var wrapper = '#!/bin/sh\n' +
            'NODE="node"\n' +
            'command -v node >/dev/null 2>&1 || NODE="' +
                process.execPath.replace(/"/g, '\\"') + '"\n' +
            'exec "$NODE" "' + source.replace(/"/g, '\\"') + '" "$@"\n';
        NodeFs.writeFileSync(target, wrapper);
        NodeFs.chmodSync(target, '755');
        console.log('[leanote] CLI installed: leanote');
    } catch (err) {
        console.log('[leanote] Auto-install failed:', err.message);
    }
})();
