// 无状态 JSON-RPC 模式测试（模拟 HARPA 浏览器扩展）
// HARPA 直接 POST JSON-RPC 到 /sse，不做 initialize 握手、不带 session-id
// 运行：node tests/test_mcp_stateless.js
"use strict";

const http = require("http");
const localServer = require("../src/local_server");

const TEST_PORT = 8916;

function post(path, body) {
  return new Promise(function (resolve, reject) {
    var data = JSON.stringify(body);
    var req = http.request(
      { hostname: "127.0.0.1", port: TEST_PORT, path: path, method: "POST",
        headers: { "Content-Type": "application/json", "Accept": "application/json, text/event-stream",
                   "Content-Length": Buffer.byteLength(data), "Origin": "chrome-extension://harpa-test" } },
      function (res) { var raw = ""; res.on("data", function (c) { raw += c; }); res.on("end", function () { resolve({ status: res.statusCode, headers: res.headers, body: raw }); }); }
    );
    req.on("error", reject);
    req.write(data); req.end();
  });
}

function parseSse(raw) {
  var m = raw.match(/data:\s*(\{[\s\S]*\})/);
  try { return m ? JSON.parse(m[1]) : null; } catch (e) { return null; }
}

async function main() {
  localServer.start(null, { mcpOnly: true, port: TEST_PORT });
  await new Promise(function (r) { setTimeout(r, 400); });

  // 1) 直接 POST tools/list（无 initialize、无 session），模拟 HARPA
  console.log("=== POST /sse tools/list (无 initialize，模拟 HARPA) ===");
  var r = await post("/sse", { jsonrpc: "2.0", id: 1, method: "tools/list" });
  console.log("status:", r.status, "| content-type:", r.headers["content-type"]);
  console.log("返回 session-id?", !!r.headers["mcp-session-id"], "(无状态不应返回)");
  var parsed = parseSse(r.body);
  var tools = parsed && parsed.result && parsed.result.tools;
  console.log("tools 数量:", tools ? tools.length : 0);
  console.log("含 add_note?", tools && tools.some(function (t) { return t.name === "add_note"; }));

  // 2) 直接 POST tools/call list_notebooks（无 initialize）
  console.log("\n=== POST /sse tools/call list_notebooks (无 initialize) ===");
  var r2 = await post("/sse", { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "list_notebooks", arguments: {} } });
  console.log("status:", r2.status);
  var parsed2 = parseSse(r2.body);
  console.log("isError?", parsed2 && parsed2.result && parsed2.result.isError);
  var text = parsed2 && parsed2.result && parsed2.result.content && parsed2.result.content[0] && parsed2.result.content[0].text;
  console.log("结果前 120 字:", text ? text.slice(0, 120) : "(none)");

  var ok = tools && tools.length >= 8 && parsed2 && !parsed2.result.isError && text;
  console.log("\n" + (ok ? "STATELESS (HARPA) OK" : "STATELESS FAIL"));
  process.exit(ok ? 0 : 2);
}
main().catch(function (e) { console.error("FAIL:", e.message); process.exit(1); });
