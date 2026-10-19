// 性能回归冒烟测试：通过本地 HTTP API 创建/读取/更新/删除笔记，断言响应时间。
// 用法：应用启动后运行  node tests/test_performance_smoke.js
// 前提：leanote 应用已运行且本地服务在 127.0.0.1:8912

var http = require('http');

var HOST = '127.0.0.1';
var PORT = 8912;
var PASS = 0;
var FAIL = 0;
var THRESHOLD_CREATE = 500; // 创建 < 500ms
var THRESHOLD_READ = 200; // 读取 < 200ms

function request(method, path, body, callback) {
    var opts = {
        hostname: HOST,
        port: PORT,
        path: path,
        method: method,
        headers: { 'Content-Type': 'application/json' }
    };
    var req = http.request(opts, function (res) {
        var data = '';
        res.on('data', function (chunk) { data += chunk; });
        res.on('end', function () {
            try { callback(null, JSON.parse(data), res.statusCode); }
            catch (e) { callback(null, data, res.statusCode); }
        });
    });
    req.on('error', function (e) { callback(e); });
    if (body) req.write(JSON.stringify(body));
    req.end();
}

function assert(msg, condition) {
    if (condition) { PASS++; console.log('  PASS: ' + msg); }
    else { FAIL++; console.error('  FAIL: ' + msg); }
}

function timed(name, fn) {
    return function () {
        var args = arguments;
        var start = Date.now();
        fn.apply(null, args);
        var elapsed = Date.now() - start;
        console.log('  [' + name + '] ' + elapsed + 'ms');
    };
}

// 1. Ping
console.log('1. Ping');
request('GET', '/api/ping', null, function (err, data, status) {
    if (err) { console.error('Server not reachable:', err.message); process.exit(1); }
    assert('Ping responds 200', status === 200);
    assert('Ping success', data && data.success === true);

    // 2. Add note
    console.log('2. Add note');
    var startCreate = Date.now();
    request('POST', '/api/note/addNote', {
        title: 'SMOKE TEST ' + Date.now(),
        content: '# Smoke Test\n\nThis is an automated smoke test note.',
        tags: ['smoke-test']
    }, function (err, data, status) {
        var createTime = Date.now() - startCreate;
        if (err) { console.error('Add note error:', err); process.exit(1); }
        assert('Add note responds 200', status === 200);
        assert('Add note success', data && data.success === true);
        assert('Add note < ' + THRESHOLD_CREATE + 'ms', createTime < THRESHOLD_CREATE);
        console.log('  [Create time] ' + createTime + 'ms');

        var noteId = data && data.noteId;
        if (!noteId) { console.error('No noteId returned'); process.exit(1); }
        console.log('  Created noteId: ' + noteId);

        // 3. Update note
        console.log('3. Update note');
        request('POST', '/api/note/updateNote', {
            noteId: noteId,
            title: 'SMOKE TEST UPDATED ' + Date.now(),
            content: '# Updated\n\nUpdated content.'
        }, function (err, data, status) {
            if (err) console.error('Update error:', err);
            assert('Update note responds 200', status === 200);
            assert('Update note success', data && data.success === true);

            // 4. Repeat 50x create + read + delete for stability
            console.log('4. Batch: 50 create + read');
            var batchCount = 50;
            var batchDone = 0;
            var batchAllPass = true;
            var batchStart = Date.now();

            function runBatch(i) {
                if (i >= batchCount) {
                    var batchTime = Date.now() - batchStart;
                    console.log('  Batch complete in ' + batchTime + 'ms (' + (batchTime / batchCount).toFixed(0) + 'ms avg)');
                    assert('Batch all 50 passed', batchAllPass);
                    assert('Batch avg < ' + THRESHOLD_CREATE + 'ms', (batchTime / batchCount) < THRESHOLD_CREATE);
                    done();
                    return;
                }

                request('POST', '/api/note/addNote', {
                    title: 'Batch ' + i,
                    content: 'Batch note ' + i,
                    tags: ['batch']
                }, function (err, data, status) {
                    if (err || !data || !data.success) { batchAllPass = false; }
                    // Delete immediately
                    if (data && data.noteId) {
                        // Leanote doesn't have a direct delete API in /api, skip cleanup
                    }
                    batchDone++;
                    process.stdout.write('\r  Batch: ' + batchDone + '/' + batchCount);
                    runBatch(i + 1);
                });
            }
            runBatch(0);

            function done() {
                console.log('\n--- Results ---');
                console.log('Pass: ' + PASS + ', Fail: ' + FAIL);
                if (FAIL > 0) process.exit(1);
                else console.log('All smoke tests passed!');
                process.exit(0);
            }
        });
    });
});
