// 共享数据访问层：直接读写 Leanote 的 NeDB 文件。
// 从 bin/leanote.js (src/leanote_cli.js) 提取的纯数据逻辑，
// 供 CLI 与 MCP Server 共用。不依赖 process.exit / console，
// 出错时抛异常，由调用方处理。

"use strict";

const path = require("path");
const os = require("os");
const crypto = require("crypto");
const fs = require("fs");

// --- 路径与上下文 ---

function getDataPath() {
  const platform = os.platform();
  if (platform === "darwin") {
    return path.join(os.homedir(), "Library", "Application Support", "leanote");
  } else if (platform === "win32") {
    return path.join(
      process.env.APPDATA || path.join(os.homedir(), "AppData", "Roaming"),
      "leanote",
    );
  }
  return path.join(os.homedir(), ".config", "leanote");
}

var _ctx = null;

function getContext() {
  if (_ctx) return _ctx;

  var dataPath = getDataPath();
  var usersDbPath = path.join(dataPath, "nedb55", "users.db");

  if (!fs.existsSync(usersDbPath)) {
    throw new Error(
      "No Leanote data found at " +
        dataPath +
        ". Please launch Leanote desktop app first to initialize data.",
    );
  }

  var users = readNDB(usersDbPath);
  var user = null;
  for (var ui = 0; ui < users.length; ui++) {
    if (users[ui].IsActive) {
      user = users[ui];
      break;
    }
  }
  if (!user) user = users[0];
  if (!user) {
    throw new Error("No user found. Please launch Leanote app and sign in first.");
  }

  var userId = user.UserId || user._id || "user1";
  var userDBPath = path.join(dataPath, "nedb55", userId);

  if (!fs.existsSync(userDBPath)) {
    throw new Error(
      "No data directory for user " + userId + ". Please launch Leanote app first.",
    );
  }

  _ctx = {
    userId: userId,
    notebooksPath: path.join(userDBPath, "notebooks.db"),
    notesPath: path.join(userDBPath, "notes.db"),
    tagsPath: path.join(userDBPath, "tags.db"),
  };
  return _ctx;
}

function resetContext() {
  _ctx = null;
}

// --- ID / 时间工具 ---

function objectId() {
  return crypto.randomBytes(12).toString("hex");
}

function nedbId() {
  return crypto
    .randomBytes(Math.ceil(Math.max(8, 16 * 2)))
    .toString("base64")
    .replace(/[+\/]/g, "")
    .slice(0, 16);
}

function nedbDate(date) {
  return { $$date: date.getTime() };
}

function parseDate(d) {
  if (!d) return 0;
  if (d && d["$$date"]) return d["$$date"];
  var ts = new Date(d).getTime();
  return isNaN(ts) ? 0 : ts;
}

// --- NeDB 文件原语 ---

function readNDB(filepath) {
  if (!fs.existsSync(filepath)) return [];
  const raw = fs.readFileSync(filepath, "utf-8").trim();
  if (!raw) return [];
  return raw
    .split("\n")
    .map(function (line) {
      try {
        return JSON.parse(line);
      } catch (e) {
        return null;
      }
    })
    .filter(Boolean);
}

function appendNDB(filepath, doc) {
  fs.appendFileSync(filepath, JSON.stringify(doc) + "\n");
}

function writeNDB(filepath, docs) {
  const tmpPath = filepath + ".tmp";
  const fd = fs.openSync(tmpPath, "w");
  docs.forEach(function (doc) {
    if (doc !== null) fs.writeSync(fd, JSON.stringify(doc) + "\n");
  });
  fs.closeSync(fd);
  fs.renameSync(tmpPath, filepath);
}

function updateNDB(filepath, matchFn, updateFn) {
  const docs = readNDB(filepath);
  writeNDB(
    filepath,
    docs.map(function (doc) {
      return matchFn(doc) ? updateFn(doc) : doc;
    }),
  );
}

// --- 笔记本查询 ---

function readNotebooks() {
  return readNDB(getContext().notebooksPath);
}

function getNotebookTitle(notebookId) {
  var notebooks = readNotebooks();
  for (var i = 0; i < notebooks.length; i++) {
    if (notebooks[i].NotebookId === notebookId) return notebooks[i].Title;
  }
  return notebookId;
}

function findNotebookByTitle(title) {
  var ctx = getContext();
  var notebooks = readNotebooks();
  for (var i = 0; i < notebooks.length; i++) {
    if (notebooks[i].Title === title && notebooks[i].UserId === ctx.userId) {
      return notebooks[i];
    }
  }
  return null;
}

// --- 笔记查询 ---

function loadNotes(filter) {
  var notes = readNDB(getContext().notesPath);
  // 按 NoteId 去重：任一副本带删除标记则整体丢弃；否则保留最新副本。
  var byId = {};
  notes.forEach(function (n) {
    var deleted = n.IsTrash === true || n.LocalIsDelete === true || n.IsDeleted === true;
    var existing = byId[n.NoteId];
    if (!existing) {
      byId[n.NoteId] = { note: n, deleted: deleted };
      return;
    }
    existing.deleted = existing.deleted || deleted;
    if (!existing.deleted && !deleted) {
      var t1 = parseDate(existing.note.UpdatedTime);
      var t2 = parseDate(n.UpdatedTime);
      if (t2 > t1) existing.note = n;
    }
  });
  var deduped = [];
  for (var k in byId) {
    if (byId[k].deleted) continue;
    deduped.push(byId[k].note);
  }
  return deduped
    .filter(
      filter ||
        function () {
          return true;
        },
    )
    .sort(function (a, b) {
      return parseDate(b.UpdatedTime) - parseDate(a.UpdatedTime);
    });
}

function findNote(noteId) {
  var notes = readNDB(getContext().notesPath);
  for (var i = 0; i < notes.length; i++) {
    if (
      notes[i].NoteId === noteId &&
      notes[i].IsTrash !== true &&
      notes[i].LocalIsDelete !== true &&
      notes[i].IsDeleted !== true
    ) {
      return notes[i];
    }
  }
  return null;
}

// 精确 → 前缀（取第一条匹配），返回完整 note 或 null
function resolveNote(noteId) {
  var note = findNote(noteId);
  if (note) return note;
  var notes = loadNotes();
  for (var i = 0; i < notes.length; i++) {
    if (notes[i].NoteId.indexOf(noteId) === 0) return notes[i];
  }
  return null;
}

// --- 操作：标签 upsert ---

function upsertTags(tags) {
  if (!tags || !tags.length) return;
  var ctx = getContext();
  var now = nedbDate(new Date());
  var existing = readNDB(ctx.tagsPath);
  for (var ti = 0; ti < tags.length; ti++) {
    var title = tags[ti];
    var found = false;
    for (var tj = 0; tj < existing.length; tj++) {
      if (existing[tj].UserId === ctx.userId && existing[tj].Tag === title) {
        found = true;
        break;
      }
    }
    if (!found) {
      appendNDB(ctx.tagsPath, {
        _id: nedbId(),
        TagId: objectId(),
        UserId: ctx.userId,
        Tag: title,
        IsDirty: true,
        Count: 1,
        LocalIsDelete: false,
        CreatedTime: now,
        UpdatedTime: now,
      });
    } else {
      updateNDB(
        ctx.tagsPath,
        function (t) {
          return t.UserId === ctx.userId && t.Tag === title;
        },
        function (t) {
          t.UpdatedTime = now;
          t.LocalIsDelete = false;
          t.IsDirty = true;
          return t;
        },
      );
    }
  }
}

// --- 操作：创建笔记 ---

function createNote(opts) {
  if (!opts.title) throw new Error("title is required");
  if (!opts.content) throw new Error("content is required");
  var ctx = getContext();
  var targetNotebook = opts.notebook || "AI札";
  var tags = opts.tags || [];

  var notebook = findNotebookByTitle(targetNotebook);
  var notebookId;
  if (notebook) {
    notebookId = notebook.NotebookId;
  } else {
    notebookId = objectId();
    appendNDB(ctx.notebooksPath, {
      _id: nedbId(),
      NotebookId: notebookId,
      Title: targetNotebook,
      Seq: -1,
      UserId: ctx.userId,
      ParentNotebookId: "",
      LocalIsNew: true,
      IsDirty: true,
    });
  }

  var noteId = objectId();
  var now = nedbDate(new Date());
  var desc = opts.content.replace(/\n/g, " ").replace(/#/g, "").substring(0, 50).trim();
  var abstract = opts.content.substring(0, 500);

  appendNDB(ctx.notesPath, {
    _id: nedbId(),
    NoteId: noteId,
    UserId: ctx.userId,
    NotebookId: notebookId,
    Title: opts.title,
    Content: opts.content,
    Desc: desc,
    ImgSrc: "",
    Tags: tags,
    Abstract: abstract,
    IsMarkdown: true,
    IsTrash: false,
    IsDirty: true,
    LocalIsNew: true,
    LocalIsDelete: false,
    IsBlog: false,
    CreatedTime: now,
    UpdatedTime: now,
  });

  upsertTags(tags);
  return { noteId: noteId, notebook: targetNotebook };
}

// --- 操作：更新笔记 ---

function updateNoteFields(opts) {
  if (!opts.noteId) throw new Error("noteId is required");
  var ctx = getContext();
  var fields = {};
  if (opts.title !== undefined) fields.Title = opts.title;
  if (opts.content !== undefined) fields.Content = opts.content;
  if (opts.tags !== undefined) fields.Tags = opts.tags;
  if (opts.notebook !== undefined) {
    var nb = findNotebookByTitle(opts.notebook);
    if (!nb) throw new Error('Notebook "' + opts.notebook + '" not found');
    fields.NotebookId = nb.NotebookId;
  }
  if (Object.keys(fields).length === 0) throw new Error("No fields to update");

  var found = false;
  updateNDB(
    ctx.notesPath,
    function (n) {
      if (n.NoteId === opts.noteId || n.NoteId.indexOf(opts.noteId) === 0) {
        if (n.IsTrash !== true && n.LocalIsDelete !== true && n.IsDeleted !== true) return true;
      }
      return false;
    },
    function (n) {
      found = true;
      for (var k in fields) n[k] = fields[k];
      if (fields.Content) {
        n.Desc = fields.Content.replace(/\n/g, " ").replace(/#/g, "").substring(0, 50).trim();
        n.Abstract = fields.Content.substring(0, 500);
      }
      n.UpdatedTime = nedbDate(new Date());
      n.IsDirty = true;
      return n;
    },
  );

  if (!found) throw new Error('Note "' + opts.noteId + '" not found');
  if (opts.tags) upsertTags(opts.tags);
  var resolved = resolveNote(opts.noteId);
  return { noteId: resolved ? resolved.NoteId : opts.noteId };
}

// --- 操作：删除（移到回收站） ---
// 参考 src/note.js 的 trash 逻辑：置 IsTrash + LocalIsDelete + IsDirty + UpdatedTime。

function deleteNote(noteId) {
  if (!noteId) throw new Error("noteId is required");
  var ctx = getContext();
  var found = false;
  var fullId = noteId;
  updateNDB(
    ctx.notesPath,
    function (n) {
      if (n.NoteId === noteId || n.NoteId.indexOf(noteId) === 0) {
        if (n.IsTrash !== true && n.LocalIsDelete !== true && n.IsDeleted !== true) return true;
      }
      return false;
    },
    function (n) {
      found = true;
      fullId = n.NoteId;
      n.IsTrash = true;
      n.LocalIsDelete = true;
      n.IsDirty = true;
      n.UpdatedTime = nedbDate(new Date());
      return n;
    },
  );
  if (!found) throw new Error('Note "' + noteId + '" not found');
  return { noteId: fullId };
}

// --- 操作：搜索 ---

function searchNotes(keyword, limit) {
  if (!keyword) throw new Error("keyword is required");
  var kw = keyword.toLowerCase();
  var notes = loadNotes();
  var results = [];

  notes.forEach(function (n) {
    var inTitle = (n.Title || "").toLowerCase().indexOf(kw) !== -1;
    var inContent = (n.Content || "").toLowerCase().indexOf(kw) !== -1;
    var inTags = (n.Tags || []).some(function (t) {
      return t.toLowerCase().indexOf(kw) !== -1;
    });
    if (inTitle || inContent || inTags) {
      var score = (inTitle ? 3 : 0) + (inTags ? 2 : 0) + (inContent ? 1 : 0);
      var content = n.Content || "";
      var idx = content.toLowerCase().indexOf(kw);
      var excerpt = "";
      if (idx !== -1) {
        var start = Math.max(0, idx - 30);
        var end = Math.min(content.length, idx + kw.length + 50);
        if (start > 0) excerpt += "...";
        excerpt += content.substring(start, end).replace(/\n/g, " ");
        if (end < content.length) excerpt += "...";
      } else {
        excerpt = (n.Desc || content).substring(0, 80);
      }
      results.push({
        noteId: n.NoteId,
        title: n.Title || "(untitled)",
        notebook: getNotebookTitle(n.NotebookId),
        tags: n.Tags || [],
        updatedTime: parseDate(n.UpdatedTime),
        score: score,
        excerpt: excerpt,
      });
    }
  });

  results.sort(function (a, b) {
    return b.score - a.score;
  });
  if (limit) results = results.slice(0, parseInt(limit, 10) || 20);
  return results;
}

// --- 操作：列出笔记 ---

function listNotes(opts) {
  opts = opts || {};
  var filter = function () {
    return true;
  };
  if (opts.notebook) {
    var nb = findNotebookByTitle(opts.notebook);
    if (!nb) throw new Error('Notebook "' + opts.notebook + '" not found');
    var nbId = nb.NotebookId;
    filter = function (n) {
      return n.NotebookId === nbId;
    };
  }
  if (opts.tag) {
    var prev = filter;
    filter = function (n) {
      return prev(n) && n.Tags && n.Tags.indexOf(opts.tag) !== -1;
    };
  }
  var notes = loadNotes(filter);
  var limit = opts.limit ? parseInt(opts.limit, 10) : 20;
  return notes.slice(0, limit).map(function (n) {
    return {
      noteId: n.NoteId,
      title: n.Title || "(untitled)",
      notebook: getNotebookTitle(n.NotebookId),
      tags: n.Tags || [],
      updatedTime: parseDate(n.UpdatedTime),
      createdTime: parseDate(n.CreatedTime),
      desc: n.Desc || "",
    };
  });
}

// --- 操作：列出笔记本 ---

function listNotebooks() {
  var ctx = getContext();
  var notes = loadNotes();
  var counts = {};
  notes.forEach(function (n) {
    counts[n.NotebookId] = (counts[n.NotebookId] || 0) + 1;
  });
  return readNotebooks()
    .filter(function (nb) {
      return nb.UserId === ctx.userId && nb.LocalIsDelete !== true && nb.IsTrash !== true;
    })
    .map(function (nb) {
      return {
        notebookId: nb.NotebookId,
        title: nb.Title,
        count: counts[nb.NotebookId] || 0,
      };
    });
}

// --- 操作：列出标签 ---

function listTags() {
  var ctx = getContext();
  return readNDB(ctx.tagsPath)
    .filter(function (t) {
      return t.UserId === ctx.userId && t.LocalIsDelete !== true;
    })
    .map(function (t) {
      return { tag: t.Tag, count: t.Count || 0 };
    });
}

// --- 操作：获取完整笔记 ---

function getNote(noteId) {
  var note = resolveNote(noteId);
  if (!note) throw new Error('Note "' + noteId + '" not found');
  return {
    noteId: note.NoteId,
    title: note.Title || "(untitled)",
    content: note.Content || "",
    notebook: getNotebookTitle(note.NotebookId),
    notebookId: note.NotebookId,
    tags: note.Tags || [],
    isMarkdown: note.IsMarkdown !== false,
    createdTime: parseDate(note.CreatedTime),
    updatedTime: parseDate(note.UpdatedTime),
  };
}

module.exports = {
  getDataPath: getDataPath,
  getContext: getContext,
  resetContext: resetContext,
  // 原语（供 CLI 复用）
  readNDB: readNDB,
  appendNDB: appendNDB,
  writeNDB: writeNDB,
  updateNDB: updateNDB,
  objectId: objectId,
  nedbId: nedbId,
  nedbDate: nedbDate,
  parseDate: parseDate,
  loadNotes: loadNotes,
  findNote: findNote,
  resolveNote: resolveNote,
  getNotebookTitle: getNotebookTitle,
  findNotebookByTitle: findNotebookByTitle,
  // 操作
  createNote: createNote,
  updateNoteFields: updateNoteFields,
  deleteNote: deleteNote,
  searchNotes: searchNotes,
  listNotes: listNotes,
  listNotebooks: listNotebooks,
  listTags: listTags,
  getNote: getNote,
};
