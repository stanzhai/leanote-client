// Byte-fixture capture: drives src/local_server.js directly (real HTTP
// server, stub services backed by in-memory rows) and records exact bytes
// for /api/ping, addNote, updateNote success+error paths. Output JSON is
// consumed by rust/crates/leanote-http/tests/api.rs parity assertions.
// Run: node .scratch/rust-rewrite/fixtures/capture_js_api.js > out.json
var localServer = require('../../../src/local_server');
var http = require('http');
var fs = require('fs');
var path = require('path');

var TEST_PORT = 8931;

// --- minimal in-memory stubs of the app services local_server uses ---
function Service(rows) { this.rows = rows; }
Service.prototype.find = function (pred, cb) {
    setTimeout(function () { cb(this.rows.filter(pred)); }.bind(this), 0);
};
Service.prototype.findOne = function (pred, cb) {
    setTimeout(function () { cb(this.rows.filter(pred)[0] || null); }.bind(this), 0);
};
Service.prototype.insert = function (doc, cb) {
    this.rows.push(doc);
    setTimeout(function () { cb(doc); }, 0);
};

var notes = [];
var notebooks = [];
var tags = [];

var noteService = {
    getNote: function (id, cb) {
        cb(notes.filter(function (n) { return n.NoteId === id; })[0] || false);
    },
    // local_server only calls updateNoteOrContent; emulate note.js:
    // new insert when IsNew and missing; else partial update of
    // Desc/ImgSrc/Title/Tags/Content + IsDirty/UpdatedTime.
    updateNoteOrContent: function (n, cb) {
        var dbNote = notes.filter(function (x) { return x.NoteId === n.NoteId; })[0];
        if (!dbNote) {
            if (!n.IsNew) { cb(false); return; }
            n.CreatedTime = new Date();
            n.UpdatedTime = new Date();
            n.IsTrash = false;
            n.LocalIsNew = true;
            delete n.IsNew;
            notes.push(n);
            if (typeof dbNotes !== 'undefined') dbNotes.insert(JSON.parse(JSON.stringify(n)));
            n.IsNew = true; // note.js re-sets it for the callback check
            cb(n);
            return;
        }
        var fields = ['Desc', 'ImgSrc', 'Title', 'Tags', 'Content'];
        var need = false;
        fields.forEach(function (f) { if (f in n) need = true; });
        if (!need) { cb(false); return; }
        var changed = false;
        if ('Content' in n && dbNote.Content !== n.Content) changed = true;
        else if ('Title' in n && dbNote.Title !== n.Title) changed = true;
        else if ('Tags' in n && (dbNote.Tags || []).join(',') !== (n.Tags || []).join(',')) changed = true;
        if (!changed) { cb(dbNote); return; }
        fields.forEach(function (f) { if (f in n) dbNote[f] = n[f]; });
        dbNote.IsDirty = true;
        dbNote.LocalIsDelete = false;
        dbNote.UpdatedTime = new Date();
        cb(n);
    },
};
var steps = [
    ['GET', '/api/ping', null],
    ['GET', '/api/nope', null],
    ['POST', '/api/note/addNote', { title: 'T', content: '# Hi\nWorld', tags: ['a', 'b'] }],
    ['POST', '/api/note/addNote', { content: 'c' }],
    ['POST', '/api/note/addNote', { title: 't' }],
    ['POST', '/api/note/addNote', '{not json'],
    ['POST', '/api/note/updateNote', { title: 'x' }],
    ['POST', '/api/note/updateNote', { noteId: 'deadbeefdeadbeefdeadbeef' }],
    // dynamic: update the note created in the add step (full id + prefix)
    function () { return ['POST', '/api/note/updateNote', { noteId: lastAddedId, title: 'Upd', content: 'body2' }]; },
    function () { return ['POST', '/api/note/updateNote', { noteId: lastAddedId.slice(0, 6), title: 'ViaPrefix' }]; },
];
var notebookService = {
    getNotebooks: function (cb) { setTimeout(function () { cb(notebooks.slice()); }, 0); },
    addNotebook: function (id, title, parent, cb) {
        var nb = { NotebookId: id, Title: title, Seq: -1, ParentNotebookId: parent, LocalIsNew: true, IsDirty: true };
        notebooks.push(nb);
        setTimeout(function () { cb(nb); }, 0);
    },
};
var tagService = {
    getTags: function (cb) { setTimeout(function () { cb(tags.slice()); }, 0); },
    addOrUpdateTag: function (title, cb) {
        tags.push({ Tag: title });
        setTimeout(function () { cb(); }, 0);
    },
};
var userService = { getCurActiveUserId: function () { return 'user1'; } };

var services = { noteService: noteService, notebookService: notebookService, tagService: tagService, userService: userService };

function req(method, path, body, cb) {
    var opts = { hostname: '127.0.0.1', port: TEST_PORT, path: path, method: method, headers: { 'Content-Type': 'application/json' } };
    var r = http.request(opts, function (res) {
        var chunks = [];
        res.on('data', function (c) { chunks.push(c); });
        res.on('end', function () {
            cb({ status: res.statusCode, headers: res.headers, body: Buffer.concat(chunks).toString() });
        });
    });
    r.on('error', function (e) { cb({ error: String(e) }); });
    if (body !== null && body !== undefined) r.write(typeof body === 'string' ? body : JSON.stringify(body));
    r.end();
}

// findNoteByPrefix's prefix fallback requires src/db.js (NeDB). Under plain
// node that needs util polyfills + a stub for require('electron'); and we
// must NEVER point it at the Electron app's real NeDB dir — so load src/db
// with an electron stub, then swap its exports for a temp-dir Datastore.
require('util').isArray = Array.isArray;
require('util').isDate = function (d) { return d instanceof Date; };
require('util').isRegExp = function (r) { return r instanceof RegExp; };
var os = require('os');
var fxDir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'leanote-js-fx-')));
var Datastore = require('nedb');
var dbNotes = new Datastore({ filename: path.join(fxDir, 'notes.db'), autoload: true });
var srcDbPath = require.resolve('../../../src/db');
var Module = require('module');
var realResolve = Module._resolveFilename;
var electronStub = path.join(fxDir, 'electron-stub.js');
fs.writeFileSync(electronStub, 'module.exports = { ipcRenderer: { on: function(){}, send: function(){} } };');
Module._resolveFilename = function (request, parent, isMain, options) {
    if (request === 'electron') return electronStub;
    return realResolve.call(this, request, parent, isMain, options);
};
require(srcDbPath);
Module._resolveFilename = realResolve;
require.cache[srcDbPath].exports = { notes: dbNotes };
localServer.start(services, { port: TEST_PORT });

var fixtures = [];
var i = 0;
var lastAddedId = null;
(function next() {
    if (i >= steps.length) {
        fs.writeFileSync(process.argv[2] || '/tmp/js_fixtures.json', JSON.stringify(fixtures, null, 2));
        process.exit(0);
    }
    var s = steps[i++];
    if (typeof s === 'function') s = s();
    req(s[0], s[1], s[2], function (resp) {
        var id = null;
        try { id = JSON.parse(resp.body).noteId; } catch (e) {}
        if (id) lastAddedId = id;
        fixtures.push({ name: s[0] + ' ' + s[1] + ' #' + (i - 1), status: resp.status, contentType: resp.headers['content-type'], allowOrigin: resp.headers['access-control-allow-origin'], body: resp.body, noteId: id });
        next();
    });
})();
