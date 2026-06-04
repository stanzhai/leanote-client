// var app = require('electron').app;  // Module to control application life.
const {app, BrowserWindow, crashReporter, Tray, Menu, ipcMain: ipc, screen} = require('electron');
var pdfMain = require('./src/pdf_main');
var appIcon;

// Report crashes to our server.
crashReporter.start({
  productName: 'Leanote',
  companyName: 'Leanote',
  submitURL: 'https://leanote.com/leanote-desktop/crash-reporter',
  autoSubmit: true
});

require('@electron/remote/main').initialize()

// Keep a global reference of the window object, if you don't, the window will
// be closed automatically when the javascript object is GCed.
var mainWindow = null;

// ---------- window state persistence ----------
const path = require('path');
const fs = require('fs');
const WIN_STATE_FILE = path.join(app.getPath('userData'), 'window-state.json');

function loadWinState() {
  try {
    return JSON.parse(fs.readFileSync(WIN_STATE_FILE, 'utf-8'));
  } catch (e) {
    return null;
  }
}

function saveWinState(bounds) {
  try {
    fs.writeFileSync(WIN_STATE_FILE, JSON.stringify({
      x: bounds.x,
      y: bounds.y,
      width: bounds.width,
      height: bounds.height,
      isMaximized: mainWindow ? mainWindow.isMaximized() : false
    }, null, 2));
  } catch (e) {}
}

function isValidPosition(state) {
  if (!state || state.width === undefined) return false;
  const displays = screen.getAllDisplays();
  // Check if at least part of the window would be visible on any display
  return displays.some(function(d) {
    var wa = d.workArea;
    // Allow the window to be at least partially visible (top-left corner within bounds)
    var xOk = state.x >= wa.x - state.width + 100 && state.x < wa.x + wa.width - 100;
    var yOk = state.y >= wa.y - state.height + 50 && state.y < wa.y + wa.height - 50;
    return xOk && yOk;
  });
}

function saveWinStateDebounced() {
  if (mainWindow && !mainWindow.isDestroyed()) {
    try { saveWinState(mainWindow.getBounds()); } catch (e) {}
  }
}
var _saveTimeout = null;
function debouncedSave() {
  clearTimeout(_saveTimeout);
  _saveTimeout = setTimeout(saveWinStateDebounced, 300);
}

if (!app.makeSingleInstance) {
  app.allowRendererProcessReuse = true

  // single instance
  const gotTheLock = app.requestSingleInstanceLock()
  if (!gotTheLock) {
    console.log("gotTheLock is false, another instance is running")
    app.quit()
  } else {
    app.on('second-instance', (event, commandLine, workingDirectory) => {
      if (mainWindow) {
        mainWindow.show();
        if (mainWindow.isMinimized()) {
          mainWindow.restore();
        }
        mainWindow.focus();
      }
    })
  }
}
else {
  // single instance
  const shouldQuit = app.makeSingleInstance((commandLine, workingDirectory) => {
    // Someone tried to run a second instance, we should focus our window.
    if (mainWindow) {
      mainWindow.show();
      if (mainWindow.isMinimized()) {
        mainWindow.restore();
      }
      mainWindow.focus();
    }
  })

  if (shouldQuit) {
    app.quit()
  }
}

// Quit when all windows are closed.
app.on('window-all-closed', function() {
  // if (process.platform != 'darwin')
    app.quit();
});

// 仅MAC
// 避免可以启动多个app
app.on('open-file', function(e) {
  // console.log('reopen');
  if(mainWindow) {
    mainWindow.show();
    mainWindow.focus();
  } else {
    openIt();
  }
});

// 仅MAC
// var appIsReady = false;
app.on('activate', function() {
  console.log('activate');
  if(mainWindow) {
    mainWindow.show();
  }
  else {
    // 有时, 重启电脑会出现这种情况
    // Cannot create BrowserWindow before app is ready
    // 很可能app.on('ready')还没有出现, 但是ready后肯定有mainWindow了
    // 所以, 这一段注释掉
    // openIt();
  }
});

// DB
var DB = {
  init: function () {
    var me = this;
    var db = require('./src/db_main');

    // 前端发来消息
    // m = {token: token, method: 'insert, findOne', dbname: 'notes', params: {username: "life"}};
    ipc.on('db-exec', function(event, m) {
      // me._token2Sender[m.token] = event.sender;
      db.exec(m, function (ret) {
        // console.log('main called ret:');
        // console.log(ret);
        if (ret && ret.ret) {
          ret.ret = JSON.stringify(ret.ret);
        }
        event.sender.send('db-exec-ret', ret);
      });
    });

    /**
     * 前端发消息过来说可以初始化了
     * @param  {<Event>} event
     * @param  {Object} params {
        curUser: <User> 是当前用户
        dbPath: string 是用户的dbPath
        dataBasePath: string 所有数据的基地址
     * }
     */
    ipc.on('db-init', function (event, params) {
      db.init(params.curUser, params.dbPath, params.dataBasePath);
    });
  }
};

// This method will be called when Electron has done everything
// initialization and ready for creating browser windows.
app.on('ready', openIt);

function removeEvents (win) {
  win.removeAllListeners('closed');
  win.removeAllListeners('focus');
  win.removeAllListeners('blur');
  win.removeAllListeners('close');
}

function close (e, force) {
  console.log('close:', force);
  if (mainWindow) {
    mainWindow.hide();
    e && e.preventDefault();
    mainWindow.webContents.send('closeWindow');
  } else {
    app.quit();
  }
}

function bindEvents (win) {
  mainWindow = win;

  // Emitted when the window is closed.
  win.on('closed', function() {
    console.log('closed');
    win = null;
  });

  win.on('focus', function() {
    console.log('focus');
    if(win && win.webContents)
      win.webContents.send('focusWindow');
  });
  win.on('blur', function() {
    console.log('blur');
    if(win && win.webContents)
      win.webContents.send('blurWindow');
  });

  // Track window position/size changes for persistence
  win.on('resize', debouncedSave);
  win.on('move', debouncedSave);
  win.on('maximize', debouncedSave);
  win.on('unmaximize', debouncedSave);

  // 以前的关闭是真关闭, 现是是假关闭了
  // 关闭,先保存数据
  win.on('close', function(e) {
    saveWinStateDebounced();

    // windows支持tray, 点close就是隐藏
    if (process.platform.toLowerCase().indexOf('win') === 0) { // win32
      win.hide();
      e.preventDefault();
      return;
    }

    // mac 在docker下quit;
    // linux直接点x linux不支持Tray
    close(e, false);
  });

}

function openIt() {
  // 数据库
  DB.init();

  // 协议
  var leanoteProtocol = require('./src/leanote_protocol');
  leanoteProtocol.init();

  var winState = loadWinState();
  var winOpts = {
      width: 1050,
      height: 595,
      frame: process.platform != 'darwin',
      transparent: false,
      autoHideMenuBar: true,
      webPreferences: {
        nodeIntegration: true,
        contextIsolation: false,
        enableRemoteModule: true
      }
    };

  // Restore previous window position/size if valid for current display setup
  if (winState && isValidPosition(winState)) {
    winOpts.x = winState.x;
    winOpts.y = winState.y;
    winOpts.width = winState.width;
    winOpts.height = winState.height;
  }

  // Create the browser window.
  mainWindow = new BrowserWindow(winOpts);

  // Restore maximized state if it was previously maximized
  if (winState && winState.isMaximized) {
    mainWindow.maximize();
  }

  // Enable @electron/remote for this window's webContents
  require('@electron/remote/main').enable(mainWindow.webContents);

  console.log('load: file://' + __dirname + '/note.html');

  // and load the index.html of the app.
  mainWindow.loadURL('file://' + __dirname + '/note.html');

  bindEvents(mainWindow);

  // When screen configuration changes, reset to default if window is off-screen
  screen.on('display-added', function() { validateWindowPosition(); });
  screen.on('display-removed', function() { validateWindowPosition(); });
  screen.on('display-metrics-changed', function() { validateWindowPosition(); });

  function validateWindowPosition() {
    if (!mainWindow || mainWindow.isDestroyed()) return;
    var bounds = mainWindow.getBounds();
    if (!isValidPosition(bounds)) {
      mainWindow.setPosition(100, 100);
      mainWindow.setSize(1050, 595);
      mainWindow.center();
    }
  }

  // 前端发来可以关闭了
  ipc.on('quit-app', function(event, arg) {
    console.log('get quit-app request');
    if (mainWindow) {
      mainWindow.destroy();
      mainWindow = null;
    } else {
      app.quit();
    }
  });

  // open login.html and note.html
  ipc.on('openUrl', function(event, arg) {
    console.log('openUrl', arg);

    // if (appIcon) {
    //     appIcon.destroy()
    // }

    arg.webPreferences = arg.webPreferences === undefined ? {} : arg.webPreferences;
    arg.webPreferences.nodeIntegration = true;
    arg.webPreferences.contextIsolation = false;
    arg.webPreferences.enableRemoteModule = true;

    var html = arg.html;
    var everWindow = mainWindow;
    if (arg.icon) {
        arg.icon = new Tray(__dirname + arg.icon)
    }
    var win2 = new BrowserWindow(arg);
    // Enable @electron/remote for this new window
    require('@electron/remote/main').enable(win2.webContents);
    win2.loadURL('file://' + __dirname + '/' + html);
    mainWindow = win2;

    // remove all events then close it
    removeEvents(everWindow);
    everWindow.close();

    if (html.indexOf('note.html') >= 0) {
      bindEvents(mainWindow)
    }
  });

  pdfMain.init();

  function show () {
    if (mainWindow) {
      mainWindow.show();
      mainWindow.restore();
      mainWindow.focus();
      mainWindow.webContents.send('focusWindow');
    } else {
      app.quit();
    }
  }

  // tray只要实例化一次
  // tray在windows下可能会有两个, 原因不明, 当注销后再启动
  var trayShowed = false;
  ipc.on('show-tray', function(event, arg) {
    if (trayShowed) {
      return;
    }
    trayShowed = true;

    if (process.platform == 'linux') {
      return;
    }

    console.log('show tray')

    appIcon = new Tray(__dirname + '/public/images/tray/' + ( process.platform == 'darwin' ? 'trayTemplate.png' : 'tray.png'))
    var contextMenu = Menu.buildFromTemplate([
      {
        label: arg.Open, click: function () {
          show();
        }
      },
      {
        label: arg.Close, click: function () {
          close(null, true);
        }
      },
    ]);
    appIcon.setToolTip('Leanote');
    // appIcon.setTitle('Leanote');
    // appIcon.setContextMenu(contextMenu);

    appIcon.on('click', function (e) {
      show();
      // e.preventDefault();
    });
    appIcon.on('right-click', function () {
      appIcon.popUpContextMenu(contextMenu);
    });

  });

}
