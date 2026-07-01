#!/usr/bin/env node

'use strict';

const path = require('path');
const os = require('os');
const crypto = require('crypto');
const fs = require('fs');

// --- Helpers ---

function getDataPath() {
    const platform = os.platform();
    if (platform === 'darwin') {
        return path.join(os.homedir(), 'Library', 'Application Support', 'leanote');
    } else if (platform === 'win32') {
        return path.join(process.env.APPDATA || path.join(os.homedir(), 'AppData', 'Roaming'), 'leanote');
    }
    return path.join(os.homedir(), '.config', 'leanote');
}

function objectId() {
    return crypto.randomBytes(12).toString('hex');
}

function readNDB(filepath) {
    if (!fs.existsSync(filepath)) return [];
    const raw = fs.readFileSync(filepath, 'utf-8').trim();
    if (!raw) return [];
    return raw.split('\n').map(function (line) {
        try { return JSON.parse(line); } catch (e) { return null; }
    }).filter(Boolean);
}

function appendNDB(filepath, doc) {
    fs.appendFileSync(filepath, JSON.stringify(doc) + '\n');
}

function writeNDB(filepath, docs) {
    const tmpPath = filepath + '.tmp';
    const fd = fs.openSync(tmpPath, 'w');
    docs.forEach(function (doc) {
        if (doc !== null) fs.writeSync(fd, JSON.stringify(doc) + '\n');
    });
    fs.closeSync(fd);
    fs.renameSync(tmpPath, filepath);
}

function updateNDB(filepath, matchFn, updateFn) {
    const docs = readNDB(filepath);
    writeNDB(filepath, docs.map(function (doc) {
        return matchFn(doc) ? updateFn(doc) : doc;
    }));
}

// --- Leanote DB access ---

var _ctx = null;

function getContext() {
    if (_ctx) return _ctx;

    var dataPath = getDataPath();
    var usersDbPath = path.join(dataPath, 'nedb55', 'users.db');

    if (!fs.existsSync(usersDbPath)) {
        console.error('Error: No Leanote data found at ' + dataPath);
        console.error('Please launch Leanote desktop app first to initialize data.');
        process.exit(1);
    }

    var users = readNDB(usersDbPath);
    var user = null;
    for (var ui = 0; ui < users.length; ui++) {
        if (users[ui].IsActive) { user = users[ui]; break; }
    }
    if (!user) user = users[0];
    if (!user) {
        console.error('Error: No user found. Please launch Leanote app and sign in first.');
        process.exit(1);
    }

    var userId = user.UserId || user._id || 'user1';
    var userDBPath = path.join(dataPath, 'nedb55', userId);

    if (!fs.existsSync(userDBPath)) {
        console.error('Error: No data directory for user ' + userId + '. Please launch Leanote app first.');
        process.exit(1);
    }

    _ctx = {
        userId: userId,
        notebooksPath: path.join(userDBPath, 'notebooks.db'),
        notesPath: path.join(userDBPath, 'notes.db'),
        tagsPath: path.join(userDBPath, 'tags.db')
    };
    return _ctx;
}

function getNotebookTitle(notebookId) {
    var ctx = getContext();
    var notebooks = readNDB(ctx.notebooksPath);
    for (var i = 0; i < notebooks.length; i++) {
        if (notebooks[i].NotebookId === notebookId) return notebooks[i].Title;
    }
    return notebookId;
}

function loadNotes(filter) {
    var ctx = getContext();
    var notes = readNDB(ctx.notesPath);
    return notes
        .filter(function (n) { return n.IsTrash !== true && n.LocalIsDelete !== true; })
        .filter(filter || function () { return true; })
        .sort(function (a, b) {
            var ta = parseDate(a.UpdatedTime);
            var tb = parseDate(b.UpdatedTime);
            return tb - ta;
        });
}

function findNote(noteId) {
    var ctx = getContext();
    var notes = readNDB(ctx.notesPath);
    for (var i = 0; i < notes.length; i++) {
        if (notes[i].NoteId === noteId && notes[i].IsTrash !== true && notes[i].LocalIsDelete !== true) {
            return notes[i];
        }
    }
    return null;
}

function parseDate(d) {
    if (!d) return 0;
    if (d && d.$$date) return d.$$date;
    var ts = new Date(d).getTime();
    return isNaN(ts) ? 0 : ts;
}

function formatTime(d) {
    var ts = parseDate(d);
    if (!ts) return '----';
    var dt = new Date(ts);
    var pad = function (n) { return n < 10 ? '0' + n : '' + n; };
    return dt.getFullYear() + '-' + pad(dt.getMonth() + 1) + '-' + pad(dt.getDate()) +
        ' ' + pad(dt.getHours()) + ':' + pad(dt.getMinutes());
}

// --- Subcommand: add ---

function cmdAdd(args) {
    var opts = parseAddArgs(args);
    if (opts._help) { printHelp('add'); process.exit(0); }

    if (!opts.title) {
        console.error('Error: --title is required.');
        process.exit(1);
    }
    if (!opts.content) {
        console.error('Error: --content is required (or pipe via stdin).');
        process.exit(1);
    }

    var ctx = getContext();

    // Find or create notebook
    var targetNotebook = opts.notebook || 'AI札';
    var notebooks = readNDB(ctx.notebooksPath);
    var notebook = null;
    for (var ni = 0; ni < notebooks.length; ni++) {
        if (notebooks[ni].Title === targetNotebook && notebooks[ni].UserId === ctx.userId) {
            notebook = notebooks[ni];
            break;
        }
    }

    var notebookId;
    if (notebook) {
        notebookId = notebook.NotebookId;
    } else {
        notebookId = objectId();
        appendNDB(ctx.notebooksPath, {
            NotebookId: notebookId,
            Title: targetNotebook,
            Seq: -1,
            UserId: ctx.userId,
            ParentNotebookId: '',
            LocalIsNew: true,
            IsDirty: true
        });
    }

    // Create note
    var noteId = objectId();
    var now = new Date();
    var desc = opts.content.replace(/\n/g, ' ').replace(/#/g, '').substring(0, 50).trim();
    var abstract = opts.content.substring(0, 500);

    appendNDB(ctx.notesPath, {
        NoteId: noteId,
        UserId: ctx.userId,
        NotebookId: notebookId,
        Title: opts.title,
        Content: opts.content,
        Desc: desc,
        ImgSrc: '',
        Tags: opts.tags,
        Abstract: abstract,
        IsMarkdown: true,
        IsTrash: false,
        IsDirty: true,
        LocalIsNew: true,
        IsBlog: false,
        CreatedTime: now,
        UpdatedTime: now
    });

    // Upsert tags
    var tags = readNDB(ctx.tagsPath);
    for (var ti = 0; ti < opts.tags.length; ti++) {
        var tagTitle = opts.tags[ti];
        var found = false;
        for (var tj = 0; tj < tags.length; tj++) {
            if (tags[tj].UserId === ctx.userId && tags[tj].Tag === tagTitle) {
                found = true;
                break;
            }
        }
        if (!found) {
            appendNDB(ctx.tagsPath, {
                TagId: objectId(),
                UserId: ctx.userId,
                Tag: tagTitle,
                IsDirty: true,
                Count: 1,
                LocalIsDelete: false,
                CreatedTime: now,
                UpdatedTime: now
            });
        } else {
            updateNDB(ctx.tagsPath,
                function (t) { return t.UserId === ctx.userId && t.Tag === tagTitle; },
                function (t) { t.UpdatedTime = now; t.LocalIsDelete = false; t.IsDirty = true; return t; }
            );
        }
    }

    var tagsStr = opts.tags.length > 0 ? ' [' + opts.tags.join(', ') + ']' : '';
    console.log('Added: "' + opts.title + '" -> ' + targetNotebook + tagsStr);
}

// --- Subcommand: list ---

function cmdList(args) {
    var opts = parseListArgs(args);
    if (opts._help) { printHelp('list'); process.exit(0); }

    var filter = function (n) { return true; };

    if (opts.notebook) {
        var ctx = getContext();
        var notebooks = readNDB(ctx.notebooksPath);
        var nbId = null;
        for (var i = 0; i < notebooks.length; i++) {
            if (notebooks[i].Title === opts.notebook) { nbId = notebooks[i].NotebookId; break; }
        }
        if (nbId) {
            filter = function (n) { return n.NotebookId === nbId; };
        } else {
            console.log('Notebook "' + opts.notebook + '" not found.');
            return;
        }
    }

    if (opts.tag) {
        var prev = filter;
        filter = function (n) { return prev(n) && n.Tags && n.Tags.indexOf(opts.tag) !== -1; };
    }

    var notes = loadNotes(filter);
    var limit = opts.limit ? parseInt(opts.limit, 10) : 20;
    var count = 0;

    notes.forEach(function (n) {
        if (count >= limit) return;
        count++;
        var notebook = getNotebookTitle(n.NotebookId);
        var tags = n.Tags && n.Tags.length > 0 ? ' [' + n.Tags.join(', ') + ']' : '';
        var title = n.Title || '(untitled)';
        // truncate title to fit terminal
        var maxTitle = 50;
        if (title.length > maxTitle) title = title.substring(0, maxTitle - 3) + '...';

        var idShort = n.NoteId.substring(0, 8);
        console.log(idShort + '  ' + formatTime(n.UpdatedTime) + '  ' + title);
        console.log('       ' + notebook + tags);

        // Show first line of content as preview when verbose
        if (opts.verbose) {
            var preview = (n.Desc || '').substring(0, 80);
            console.log('       ' + preview);
        }
    });

    if (count === 0) {
        console.log('No notes found.');
    } else if (notes.length > limit) {
        console.log('... (' + (notes.length - limit) + ' more, use -N to show more)');
    }
}

// --- Subcommand: show ---

function cmdShow(args) {
    var opts = parseShowArgs(args);
    if (opts._help) { printHelp('show'); process.exit(0); }

    if (!opts.noteId) {
        console.error('Error: <note-id> is required.');
        process.exit(1);
    }

    var note = findNote(opts.noteId);
    if (!note) {
        // try prefix match
        var notes = loadNotes();
        for (var i = 0; i < notes.length; i++) {
            if (notes[i].NoteId.indexOf(opts.noteId) === 0) { note = notes[i]; break; }
        }
    }

    if (!note) {
        console.error('Error: Note "' + opts.noteId + '" not found.');
        process.exit(1);
    }

    var notebook = getNotebookTitle(note.NotebookId);
    var tags = note.Tags && note.Tags.length > 0 ? note.Tags.join(', ') : '(none)';

    console.log('ID:       ' + note.NoteId);
    console.log('Title:    ' + (note.Title || '(untitled)'));
    console.log('Notebook: ' + notebook);
    console.log('Tags:     ' + tags);
    console.log('Created:  ' + formatTime(note.CreatedTime));
    console.log('Updated:  ' + formatTime(note.UpdatedTime));
    console.log('---');
    console.log(note.Content || '(empty)');
}

// --- Subcommand: edit ---

function cmdEdit(args) {
    var opts = parseEditArgs(args);
    if (opts._help) { printHelp('edit'); process.exit(0); }

    if (!opts.noteId) {
        console.error('Error: <note-id> is required.');
        process.exit(1);
    }

    var ctx = getContext();
    var fields = {};

    if (opts.title !== undefined) fields.Title = opts.title;
    if (opts.content !== undefined) fields.Content = opts.content;
    if (opts.tags !== undefined) fields.Tags = opts.tags;
    if (opts.notebook !== undefined) {
        var notebooks = readNDB(ctx.notebooksPath);
        for (var i = 0; i < notebooks.length; i++) {
            if (notebooks[i].Title === opts.notebook) {
                fields.NotebookId = notebooks[i].NotebookId;
                break;
            }
        }
        if (!fields.NotebookId) {
            console.error('Error: Notebook "' + opts.notebook + '" not found.');
            process.exit(1);
        }
    }

    if (Object.keys(fields).length === 0) {
        console.error('Error: No fields to update. Use --title, --content, --tags, or --notebook.');
        process.exit(1);
    }

    var found = false;
    updateNDB(ctx.notesPath,
        function (n) {
            if (n.NoteId === opts.noteId || n.NoteId.indexOf(opts.noteId) === 0) {
                if (n.IsTrash !== true && n.LocalIsDelete !== true) {
                    return true;
                }
            }
            return false;
        },
        function (n) {
            found = true;
            for (var k in fields) n[k] = fields[k];
            if (fields.Content) {
                n.Desc = fields.Content.replace(/\n/g, ' ').replace(/#/g, '').substring(0, 50).trim();
                n.Abstract = fields.Content.substring(0, 500);
            }
            n.UpdatedTime = new Date();
            n.IsDirty = true;
            return n;
        }
    );

    if (!found) {
        console.error('Error: Note "' + opts.noteId + '" not found.');
        process.exit(1);
    }

    // Upsert tags if changed
    if (opts.tags) {
        var tags = readNDB(ctx.tagsPath);
        for (var ti = 0; ti < opts.tags.length; ti++) {
            var tagTitle = opts.tags[ti];
            var tagFound = false;
            for (var tj = 0; tj < tags.length; tj++) {
                if (tags[tj].UserId === ctx.userId && tags[tj].Tag === tagTitle) {
                    tagFound = true;
                    break;
                }
            }
            if (!tagFound) {
                appendNDB(ctx.tagsPath, {
                    TagId: objectId(),
                    UserId: ctx.userId,
                    Tag: tagTitle,
                    IsDirty: true,
                    Count: 1,
                    LocalIsDelete: false,
                    CreatedTime: new Date(),
                    UpdatedTime: new Date()
                });
            }
        }
    }

    console.log('Updated: ' + (opts.noteId.length < 20 ? opts.noteId + '...' : opts.noteId.substring(0, 8) + '...'));
}

// --- Subcommand: search ---

function cmdSearch(args) {
    var opts = parseSearchArgs(args);
    if (opts._help) { printHelp('search'); process.exit(0); }

    if (!opts.keyword) {
        console.error('Error: <keyword> is required.');
        process.exit(1);
    }

    var kw = opts.keyword.toLowerCase();
    var notes = loadNotes();
    var results = [];

    notes.forEach(function (n) {
        var inTitle = (n.Title || '').toLowerCase().indexOf(kw) !== -1;
        var inContent = (n.Content || '').toLowerCase().indexOf(kw) !== -1;
        var inTags = (n.Tags || []).some(function (t) { return t.toLowerCase().indexOf(kw) !== -1; });

        if (inTitle || inContent || inTags) {
            var score = (inTitle ? 3 : 0) + (inTags ? 2 : 0) + (inContent ? 1 : 0);
            results.push({ note: n, score: score });
        }
    });

    results.sort(function (a, b) { return b.score - a.score; });

    var limit = opts.limit ? parseInt(opts.limit, 10) : 20;
    var count = 0;

    results.forEach(function (r) {
        if (count >= limit) return;
        count++;
        var n = r.note;
        var notebook = getNotebookTitle(n.NotebookId);
        var tags = n.Tags && n.Tags.length > 0 ? ' [' + n.Tags.join(', ') + ']' : '';
        var title = n.Title || '(untitled)';

        // Highlight matching keyword
        var idShort = n.NoteId.substring(0, 8);
        console.log(idShort + '  ' + formatTime(n.UpdatedTime) + '  ' + title);

        // Show content excerpt with keyword context
        var content = n.Content || '';
        var idx = content.toLowerCase().indexOf(kw);
        var excerpt = '';
        if (idx !== -1) {
            var start = Math.max(0, idx - 30);
            var end = Math.min(content.length, idx + kw.length + 50);
            if (start > 0) excerpt += '...';
            excerpt += content.substring(start, end).replace(/\n/g, ' ');
            if (end < content.length) excerpt += '...';
        } else {
            excerpt = (n.Desc || content).substring(0, 80);
        }
        console.log('       ' + excerpt);
        console.log('       ' + notebook + tags);
    });

    if (count === 0) {
        console.log('No notes matching "' + opts.keyword + '".');
    } else if (results.length > limit) {
        console.log('... (' + (results.length - limit) + ' more, use -N to show more)');
    }
}

// --- Arg parsing ---

function parseAddArgs(argv) {
    var opts = { title: '', content: '', tags: [], notebook: '', _readStdin: false };
    var i = 0;
    while (i < argv.length) {
        var arg = argv[i];
        if (arg === '--title' || arg === '-t') {
            opts.title = argv[++i] || '';
        } else if (arg === '--content' || arg === '-c') {
            opts.content = argv[++i] || '';
        } else if (arg === '--tags' || arg === '-g') {
            opts.tags = (argv[++i] || '').split(',').map(function (t) { return t.trim(); }).filter(Boolean);
        } else if (arg === '--notebook' || arg === '-n') {
            opts.notebook = argv[++i] || '';
        } else if (arg === '--help' || arg === '-h') {
            opts._help = true;
        }
        i++;
    }
    if (!opts.content && !process.stdin.isTTY) {
        opts._readStdin = true;
    }
    return opts;
}

function parseListArgs(argv) {
    var opts = { notebook: '', tag: '', limit: 20, verbose: false };
    var i = 0;
    while (i < argv.length) {
        var arg = argv[i];
        if (arg === '--notebook' || arg === '-n') {
            opts.notebook = argv[++i] || '';
        } else if (arg === '--tag' || arg === '-g') {
            opts.tag = argv[++i] || '';
        } else if (arg === '--limit' || arg === '-N') {
            opts.limit = parseInt(argv[++i], 10) || 20;
        } else if (arg === '--verbose' || arg === '-v') {
            opts.verbose = true;
        } else if (arg === '--help' || arg === '-h') {
            opts._help = true;
        }
        i++;
    }
    return opts;
}

function parseShowArgs(argv) {
    var opts = { noteId: '' };
    var i = 0;
    while (i < argv.length) {
        var arg = argv[i];
        if (arg === '--help' || arg === '-h') {
            opts._help = true;
        } else if (arg[0] !== '-') {
            opts.noteId = arg;
        }
        i++;
    }
    return opts;
}

function parseEditArgs(argv) {
    var opts = { noteId: '', title: undefined, content: undefined, tags: undefined, notebook: undefined };
    var i = 0;
    while (i < argv.length) {
        var arg = argv[i];
        if (arg === '--title' || arg === '-t') {
            opts.title = argv[++i] || '';
        } else if (arg === '--content' || arg === '-c') {
            opts.content = argv[++i] || '';
        } else if (arg === '--tags' || arg === '-g') {
            opts.tags = (argv[++i] || '').split(',').map(function (t) { return t.trim(); }).filter(Boolean);
        } else if (arg === '--notebook' || arg === '-n') {
            opts.notebook = argv[++i] || '';
        } else if (arg === '--help' || arg === '-h') {
            opts._help = true;
        } else if (arg[0] !== '-') {
            opts.noteId = arg;
        }
        i++;
    }
    return opts;
}

function parseSearchArgs(argv) {
    var opts = { keyword: '', limit: 20 };
    var i = 0;
    while (i < argv.length) {
        var arg = argv[i];
        if (arg === '--limit' || arg === '-N') {
            opts.limit = parseInt(argv[++i], 10) || 20;
        } else if (arg === '--help' || arg === '-h') {
            opts._help = true;
        } else if (arg[0] !== '-') {
            opts.keyword = arg;
        }
        i++;
    }
    return opts;
}

// --- Help ---

function printHelp(cmd) {
    if (cmd === 'add' || !cmd) {
        console.log('Usage: leanote add [options]');
        console.log('       leanote [options]                  (add is the default)');
        console.log('');
        console.log('Add a markdown note to Leanote.');
        console.log('');
        console.log('Options:');
        console.log('  -t, --title <title>       Note title (required)');
        console.log('  -c, --content <content>   Markdown content (use -c or pipe from stdin)');
        console.log('  -g, --tags <tags>         Comma-separated tags');
        console.log('  -n, --notebook <notebook>  Target notebook (default: "AI札")');
        console.log('  -h, --help                Show this help');
        console.log('');
        console.log('Examples:');
        console.log('  leanote add -t "Meeting" -c "# Notes" -g "work"');
        console.log('  leanote -t "Quick" -c "content"');
        console.log('  echo "# Hello" | leanote -t "Pipe test"');
    } else if (cmd === 'list') {
        console.log('Usage: leanote list [options]');
        console.log('');
        console.log('List recent notes.');
        console.log('');
        console.log('Options:');
        console.log('  -n, --notebook <name>    Filter by notebook');
        console.log('  -g, --tag <tag>          Filter by tag');
        console.log('  -N, --limit <n>          Max results (default: 20)');
        console.log('  -v, --verbose            Show content preview');
        console.log('  -h, --help               Show this help');
        console.log('');
        console.log('Examples:');
        console.log('  leanote list');
        console.log('  leanote list -n "AI札" -v');
        console.log('  leanote list -g "work" -N 5');
    } else if (cmd === 'show') {
        console.log('Usage: leanote show <note-id>');
        console.log('');
        console.log('Show full content of a note. <note-id> can be a prefix.');
        console.log('');
        console.log('Examples:');
        console.log('  leanote show a1b2c3d4');
        console.log('  leanote show a1b2');
    } else if (cmd === 'search') {
        console.log('Usage: leanote search <keyword> [options]');
        console.log('');
        console.log('Search notes by keyword (title, content, tags).');
        console.log('');
        console.log('Options:');
        console.log('  -N, --limit <n>          Max results (default: 20)');
        console.log('  -h, --help               Show this help');
        console.log('');
        console.log('Examples:');
        console.log('  leanote search "meeting"');
        console.log('  leanote search "project" -N 10');
    } else if (cmd === 'edit') {
        console.log('Usage: leanote edit <note-id> [options]');
        console.log('');
        console.log('Edit an existing note. <note-id> can be a prefix.');
        console.log('At least one option must be provided.');
        console.log('');
        console.log('Options:');
        console.log('  -t, --title <title>       New title');
        console.log('  -c, --content <content>   New markdown content (use -c or pipe from stdin)');
        console.log('  -g, --tags <tags>         New comma-separated tags (replaces existing)');
        console.log('  -n, --notebook <name>     Move to notebook');
        console.log('  -h, --help                Show this help');
        console.log('');
        console.log('Examples:');
        console.log('  leanote edit a1b2c3d4 -t "New Title"');
        console.log('  leanote edit a1b2 -g "urgent,work"');
        console.log('  echo "Updated content" | leanote edit a1b2 -c -');
    }
}

function printGlobalHelp() {
    console.log('Usage: leanote <command> [options]');
    console.log('');
    console.log('Commands:');
    console.log('  add       Add a new note (default if no command given)');
    console.log('  list      List recent notes');
    console.log('  show      Show full note content by ID');
    console.log('  search    Search notes by keyword');
    console.log('  edit      Edit an existing note');
    console.log('');
    console.log('Run "leanote <command> --help" for command-specific help.');
    console.log('');
    console.log('Examples:');
    console.log('  leanote -t "Note" -c "Content" -g "tag1,tag2"');
    console.log('  leanote list -n "AI札"');
    console.log('  leanote search "keyword"');
    console.log('  leanote show a1b2c3d4');
    console.log('  leanote edit a1b2c3d4 -t "New title"');
}

function readStdin() {
    return new Promise(function (resolve) {
        var data = '';
        process.stdin.setEncoding('utf8');
        process.stdin.on('readable', function () {
            var chunk;
            while ((chunk = process.stdin.read()) !== null) data += chunk;
        });
        process.stdin.on('end', function () { resolve(data.trim()); });
        process.stdin.resume();
    });
}

// --- Main ---

async function main() {
    var argv = process.argv.slice(2);

    // Determine subcommand
    var cmd = 'add';
    var cmdIndex = -1;

    var subcommands = ['add', 'list', 'show', 'search', 'edit', 'help'];

    for (var i = 0; i < argv.length; i++) {
        if (subcommands.indexOf(argv[i]) !== -1) {
            cmd = argv[i];
            cmdIndex = i;
            break;
        }
    }

    if (cmd === 'help') {
        if (argv.length > 1 && subcommands.indexOf(argv[1]) !== -1) {
            printHelp(argv[1]);
        } else {
            printGlobalHelp();
        }
        process.exit(0);
    }

    // Global --help / -h (before subcommand dispatch)
    if (argv.indexOf('--help') !== -1 || argv.indexOf('-h') !== -1) {
        if (cmdIndex >= 0) {
            printHelp(cmd);
        } else {
            printGlobalHelp();
        }
        process.exit(0);
    }

    // Remove subcommand from args
    var cmdArgs = cmdIndex >= 0 ? argv.slice(cmdIndex + 1) : argv;

    // Read stdin if needed (for add and edit)
    var pipedContent = '';
    if (!process.stdin.isTTY) {
        pipedContent = await readStdin();
    }

    switch (cmd) {
        case 'add':
            if (pipedContent && !cmdArgs.some(function (a) { return a === '-c' || a === '--content'; })) {
                // Use piped content when no explicit -c given
                var addOpts = parseAddArgs(cmdArgs);
                if (addOpts._help) { printHelp('add'); process.exit(0); }
                addOpts.content = pipedContent;
                addOpts._readStdin = false;
                cmdAddInternal(addOpts);
            } else {
                var opts = parseAddArgs(cmdArgs);
                if (opts._help) { printHelp('add'); process.exit(0); }
                if (opts._readStdin) opts.content = pipedContent;
                cmdAddInternal(opts);
            }
            break;
        case 'list':
            cmdList(cmdArgs);
            break;
        case 'show':
            cmdShow(cmdArgs);
            break;
        case 'search':
            cmdSearch(cmdArgs);
            break;
        case 'edit':
            if (pipedContent) {
                var editOpts = parseEditArgs(cmdArgs);
                if (editOpts._help) { printHelp('edit'); process.exit(0); }
                editOpts.content = pipedContent;
                cmdEditInternal(editOpts);
            } else {
                cmdEdit(cmdArgs);
            }
            break;
        default:
            console.error('Unknown command: ' + cmd);
            printGlobalHelp();
            process.exit(1);
    }
}

// Internal wrappers that bypass the cmd* parse helpers
function cmdAddInternal(opts) {
    if (!opts.title) { console.error('Error: --title is required.'); process.exit(1); }
    if (!opts.content) { console.error('Error: --content is required (or pipe via stdin).'); process.exit(1); }

    var ctx = getContext();
    var targetNotebook = opts.notebook || 'AI札';
    var notebooks = readNDB(ctx.notebooksPath);
    var notebook = null;
    for (var ni = 0; ni < notebooks.length; ni++) {
        if (notebooks[ni].Title === targetNotebook && notebooks[ni].UserId === ctx.userId) {
            notebook = notebooks[ni]; break;
        }
    }
    var notebookId;
    if (notebook) {
        notebookId = notebook.NotebookId;
    } else {
        notebookId = objectId();
        appendNDB(ctx.notebooksPath, {
            NotebookId: notebookId, Title: targetNotebook, Seq: -1,
            UserId: ctx.userId, ParentNotebookId: '', LocalIsNew: true, IsDirty: true
        });
    }

    var noteId = objectId();
    var now = new Date();
    var desc = opts.content.replace(/\n/g, ' ').replace(/#/g, '').substring(0, 50).trim();
    var abstract = opts.content.substring(0, 500);

    appendNDB(ctx.notesPath, {
        NoteId: noteId, UserId: ctx.userId, NotebookId: notebookId,
        Title: opts.title, Content: opts.content, Desc: desc, ImgSrc: '',
        Tags: opts.tags, Abstract: abstract, IsMarkdown: true, IsTrash: false,
        IsDirty: true, LocalIsNew: true, IsBlog: false, CreatedTime: now, UpdatedTime: now
    });

    var tags = readNDB(ctx.tagsPath);
    for (var ti = 0; ti < opts.tags.length; ti++) {
        var tagTitle = opts.tags[ti];
        var found = false;
        for (var tj = 0; tj < tags.length; tj++) {
            if (tags[tj].UserId === ctx.userId && tags[tj].Tag === tagTitle) { found = true; break; }
        }
        if (!found) {
            appendNDB(ctx.tagsPath, {
                TagId: objectId(), UserId: ctx.userId, Tag: tagTitle,
                IsDirty: true, Count: 1, LocalIsDelete: false, CreatedTime: now, UpdatedTime: now
            });
        } else {
            updateNDB(ctx.tagsPath,
                function (t) { return t.UserId === ctx.userId && t.Tag === tagTitle; },
                function (t) { t.UpdatedTime = now; t.LocalIsDelete = false; t.IsDirty = true; return t; }
            );
        }
    }

    var tagsStr = opts.tags.length > 0 ? ' [' + opts.tags.join(', ') + ']' : '';
    console.log('Added: "' + opts.title + '" -> ' + targetNotebook + tagsStr);
}

function cmdEditInternal(opts) {
    if (!opts.noteId) { console.error('Error: <note-id> is required.'); process.exit(1); }
    var ctx = getContext();
    var fields = {};
    if (opts.title !== undefined) fields.Title = opts.title;
    if (opts.content !== undefined) fields.Content = opts.content;
    if (opts.tags !== undefined) fields.Tags = opts.tags;
    if (opts.notebook !== undefined) {
        var notebooks = readNDB(ctx.notebooksPath);
        for (var i = 0; i < notebooks.length; i++) {
            if (notebooks[i].Title === opts.notebook) { fields.NotebookId = notebooks[i].NotebookId; break; }
        }
        if (!fields.NotebookId) { console.error('Error: Notebook "' + opts.notebook + '" not found.'); process.exit(1); }
    }
    if (Object.keys(fields).length === 0) { console.error('Error: No fields to update.'); process.exit(1); }

    var found = false;
    updateNDB(ctx.notesPath,
        function (n) {
            if (n.NoteId === opts.noteId || n.NoteId.indexOf(opts.noteId) === 0) {
                if (n.IsTrash !== true && n.LocalIsDelete !== true) return true;
            }
            return false;
        },
        function (n) {
            found = true;
            for (var k in fields) n[k] = fields[k];
            if (fields.Content) {
                n.Desc = fields.Content.replace(/\n/g, ' ').replace(/#/g, '').substring(0, 50).trim();
                n.Abstract = fields.Content.substring(0, 500);
            }
            n.UpdatedTime = new Date();
            n.IsDirty = true;
            return n;
        }
    );
    if (!found) { console.error('Error: Note "' + opts.noteId + '" not found.'); process.exit(1); }
    if (opts.tags) {
        var tags = readNDB(ctx.tagsPath);
        for (var ti = 0; ti < opts.tags.length; ti++) {
            var tagTitle = opts.tags[ti], tagFound = false;
            for (var tj = 0; tj < tags.length; tj++) {
                if (tags[tj].UserId === ctx.userId && tags[tj].Tag === tagTitle) { tagFound = true; break; }
            }
            if (!tagFound) {
                appendNDB(ctx.tagsPath, { TagId: objectId(), UserId: ctx.userId, Tag: tagTitle, IsDirty: true, Count: 1, LocalIsDelete: false, CreatedTime: new Date(), UpdatedTime: new Date() });
            }
        }
    }
    console.log('Updated: ' + (opts.noteId.substring(0, 8) + '...'));
}

main().catch(function (err) {
    console.error('Error:', err.message);
    process.exit(1);
});
