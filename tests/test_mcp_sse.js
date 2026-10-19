// SSE 传输冒烟测试（旧版 HTTP+SSE 协议，模拟浏览器扩展等老客户端）
// 运行：node tests/test_mcp_sse.js
// 流程：GET /sse 拿 endpoint → POST /messages 发 initialize/tools/list，响应通过 SSE 流返回

"use strict";

const http = require("http");
const localServer = require("../src/local_server");

const TEST_PORT = 8915;

function sleep(ms) { return new Promise(function (r) { setTimeout(r, ms); }); }

function postMsg(path, body) {
  return new Promise(function (resolve, reject) {
    var data = JSON.stringify(body);
    var req = http.request(
      { hostname: "127.0.0.1", port: TEST_PORT, path: path, method: "POST",
        headers: { "Content-Type": "application/json", "Content-Length": Buffer.byteLength(data) } },
      function (res) {
        var raw = "";
        res.on("data", function (c) { raw += c; });
        res.on("end", function () { resolve({ status: res.statusCode, body: raw }); });
      },
    );
    req.on("error", reject);
    req.write(data);
    req.end();
  });
}

async function main() {
  localServer.start(null, { mcpOnly: true, port: TEST_PORT });
  await sleep(400);

  // 1) GET /sse 建立长连接，监听 endpoint event 与后续 message
  var endpoint = null;
  var messages = [];
  await new Promise(function (resolve) {
    var req = http.get(
      { hostname: "127.0.0.1", port: TEST_PORT, path: "/sse", headers: { Accept: "text/event-stream" } },
      function (res) {
        var buf = "";
        res.on("data", function (c) {
          buf += c.toString();
          var em = buf.match(/event:\s*endpoint\ndata:\s*([^\r\n]+)/);
          if (em && !endpoint) endpoint = em[1].trim();
          buf.split(/\n\n/).forEach(function (block) {
            var mm = block.match(/event:\s*message\ndata:\s*(\{.*\})/s);
            if (mm && messages.indexOf(mm[1]) === -1) messages.push(mm[1]);
          });
        });
        setTimeout(resolve, 2500);
      },
    );
    req.on("error", function () { resolve(); });
  });

  console.log("SSE endpoint:", endpoint);
  if (!endpoint || endpoint.indexOf("/messages?sessionId=") === -1) {
    throw new Error("No valid SSE endpoint received");
  }

  // 2) initialize
  var r = await postMsg(endpoint, {
    jsonrpc: "2.0", id: 1, method: "initialize",
    params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "sse-test", version: "1.0" } },
  });
  console.log("initialize POST status:", r.status);
  await sleep(500);

  // 3) notifications/initialized
  r = await postMsg(endpoint, { jsonrpc: "2.0", method: "notifications/initialized" });
  console.log("initialized POST status:", r.status);
  await sleep(300);

  // 4) tools/list
  r = await postMsg(endpoint, { jsonrpc: "2.0", id: 2, method: "tools/list" });
  console.log("tools/list POST status:", r.status);
  await sleep(500);

  console.log("messages received via SSE:", messages.length);
  messages.forEach(function (m, i) { console.log("  [" + i + "]", m.slice(0, 180)); });

  var hasInit = messages.some(function (m) { return m.indexOf("serverInfo") !== -1; });
  var hasTools = messages.some(function (m) { return m.indexOf('"tools"') !== -1; });
  console.log("initialize response via SSE:", hasInit);
  console.log("tools/list response via SSE:", hasTools);

  if (hasInit && hasTools) {
    console.log("\nSSE TRANSPORT OK");
  } else {
    console.log("\nSSE TRANSPORT INCOMPLETE");
    process.exit(2);
  }
  process.exit(0);
}

main().catch(function (e) {
  console.error("TEST FAILED:", e.message);
  process.exit(1);
});
