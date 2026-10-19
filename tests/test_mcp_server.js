// 独立冒烟测试：用 mcpOnly 模式启动 local_server 的 /mcp 路由，模拟 MCP 客户端验证。
// 生产环境 /mcp 与 /api/* 共用 8912；此处用 8913 仅作测试隔离，避免与运行中的 app 冲突。
// 运行：node tests/test_mcp_server.js   （需本机已有 Leanote 数据）

"use strict";

const http = require("http");
const localServer = require("../src/local_server");

const TEST_PORT = 8913;

function req(method, body, sessionId) {
  return new Promise(function (resolve, reject) {
    var data = body ? JSON.stringify(body) : "";
    var headers = {
      "Content-Type": "application/json",
      Accept: "application/json, text/event-stream",
    };
    if (sessionId) headers["Mcp-Session-Id"] = sessionId;
    var r = http.request(
      { hostname: "127.0.0.1", port: TEST_PORT, path: "/mcp", method: method, headers: headers },
      function (res) {
        var raw = "";
        res.on("data", function (c) { raw += c; });
        res.on("end", function () {
          resolve({ status: res.statusCode, headers: res.headers, body: raw });
        });
      },
    );
    r.on("error", reject);
    r.write(data);
    r.end();
  });
}

// 响应可能是单 JSON 或 SSE 流，统一解析为 JSON 对象
function parseBody(raw) {
  var trimmed = (raw || "").trim();
  if (!trimmed) return null;
  if (trimmed.indexOf("event:") === 0 || trimmed.indexOf("data:") === 0) {
    var payload = "";
    trimmed.split("\n").forEach(function (l) {
      if (l.indexOf("data:") === 0) payload += l.slice(5).trim();
    });
    try { return JSON.parse(payload); } catch (e) { return payload; }
  }
  try { return JSON.parse(trimmed); } catch (e) { return trimmed; }
}

function log(label, obj) {
  console.log("---- " + label + " ----");
  console.log(typeof obj === "string" ? obj : JSON.stringify(obj, null, 2));
}

async function main() {
  localServer.start(null, { mcpOnly: true, port: TEST_PORT });
  await new Promise(function (r) { setTimeout(r, 400); });

  // 1) initialize
  var res = await req("POST", {
    jsonrpc: "2.0", id: 1, method: "initialize",
    params: {
      protocolVersion: "2025-06-18",
      capabilities: {},
      clientInfo: { name: "leanote-mcp-test", version: "1.0.0" },
    },
  });
  console.log("initialize status:", res.status);
  var sid = res.headers["mcp-session-id"];
  console.log("session id:", sid);
  if (!sid) { console.log("BODY:", res.body); throw new Error("No session id returned"); }
  var init = parseBody(res.body);
  console.log("serverInfo:", init && init.result && init.result.serverInfo);

  // 2) initialized notification
  res = await req("POST", { jsonrpc: "2.0", method: "notifications/initialized" }, sid);
  console.log("initialized notification status:", res.status);

  // 3) tools/list
  res = await req("POST", { jsonrpc: "2.0", id: 2, method: "tools/list" }, sid);
  var toolsList = parseBody(res.body);
  var tools = (toolsList.result && toolsList.result.tools) || [];
  console.log("tools count:", tools.length);
  console.log("tool names:", tools.map(function (t) { return t.name; }).join(", "));
  console.log("add_note inputSchema present:", !!(tools.filter(function (t) { return t.name === "add_note"; })[0] || {}).inputSchema);

  // 4) tools/call list_notebooks
  res = await req("POST", { jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "list_notebooks", arguments: {} } }, sid);
  var r1 = parseBody(res.body);
  log("list_notebooks (first 3)", JSON.parse(r1.result.content[0].text).slice(0, 3));

  // 5) tools/call search_notes
  res = await req("POST", { jsonrpc: "2.0", id: 4, method: "tools/call", params: { name: "search_notes", arguments: { keyword: "LLM", limit: 3 } } }, sid);
  var r2 = parseBody(res.body);
  log("search_notes 'LLM'", JSON.parse(r2.result.content[0].text));

  // 6) tools/call get_note (error path: bad id)
  res = await req("POST", { jsonrpc: "2.0", id: 5, method: "tools/call", params: { name: "get_note", arguments: { noteId: "nonexistent_id_xyz" } } }, sid);
  var r3 = parseBody(res.body);
  log("get_note (bad id, expect isError)", r3.result);

  // 6.5) write ops loop: add → get → update → delete (自清理，最终进回收站)
  res = await req("POST", { jsonrpc: "2.0", id: 6, method: "tools/call", params: { name: "add_note", arguments: { title: "[MCP测试] 临时笔记", content: "# 测试\nMCP server 自动化测试创建。", tags: ["_mcp_test"], notebook: "AI札" } } }, sid);
  var rAdd = parseBody(res.body);
  var added = JSON.parse(rAdd.result.content[0].text);
  log("add_note", added);
  var newId = added.noteId;

  res = await req("POST", { jsonrpc: "2.0", id: 7, method: "tools/call", params: { name: "get_note", arguments: { noteId: newId } } }, sid);
  log("get_note (new)", JSON.parse(parseBody(res.body).result.content[0].text));

  res = await req("POST", { jsonrpc: "2.0", id: 8, method: "tools/call", params: { name: "update_note", arguments: { noteId: newId, title: "[MCP测试] 已更新", content: "# 已更新内容" } } }, sid);
  log("update_note", JSON.parse(parseBody(res.body).result.content[0].text));

  res = await req("POST", { jsonrpc: "2.0", id: 9, method: "tools/call", params: { name: "delete_note", arguments: { noteId: newId } } }, sid);
  log("delete_note", JSON.parse(parseBody(res.body).result.content[0].text));

  // 删除后应找不到（findNote 过滤 IsTrash）
  res = await req("POST", { jsonrpc: "2.0", id: 10, method: "tools/call", params: { name: "get_note", arguments: { noteId: newId } } }, sid);
  log("get_note (after delete, expect isError)", parseBody(res.body).result);

  // 7) delete session
  res = await req("DELETE", null, sid);
  console.log("delete session status:", res.status);

  console.log("\nALL CHECKS PASSED");
  process.exit(0);
}

main().catch(function (e) {
  console.error("TEST FAILED:", e.message);
  process.exit(1);
});
