//! Dev 钩子：`LEANOTE_FAKE_SERVER=1` 起进程内 fake server（桌面同款约定）
//! 并自动登录——模拟器冒烟无法程序化键入，靠它走真实登录路径。

use serde_json::json;

/// Boot fake server on its own thread + runtime; the URL arrives on the
/// returned receiver once the listener is bound.
pub fn boot_fake_server() -> std::sync::mpsc::Receiver<String> {
    use std::sync::mpsc::channel;
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio rt");
        rt.block_on(async {
            let server = leanote_fakeserver::start().await;
            server.set_sync_usn(20);
            server.set_notebooks(vec![
                json!({"NotebookId":"nb1","Title":"Inbox","Seq":1,"ParentNotebookId":"","IsDeleted":false,"Usn":1,"CreatedTime":"2026-01-02T10:00:00","UpdatedTime":"2026-01-02T10:00:00"}),
                json!({"NotebookId":"nb2","Title":"Work","Seq":2,"ParentNotebookId":"","IsDeleted":false,"Usn":2,"CreatedTime":"2026-01-03T10:00:00","UpdatedTime":"2026-01-03T10:00:00"}),
                json!({"NotebookId":"nb2a","Title":"Research","Seq":1,"ParentNotebookId":"nb2","IsDeleted":false,"Usn":5,"CreatedTime":"2026-01-03T11:30:00","UpdatedTime":"2026-01-03T11:30:00"}),
            ]);
            server.set_notes(vec![
                json!({"NoteId":"n1","Title":"Welcome to Leanote","NotebookId":"nb1","Usn":4,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-05T10:00:00","UpdatedTime":"2026-01-05T10:00:00"}),
                json!({"NoteId":"n2","Title":"Rust workspace layout","NotebookId":"nb2","Usn":5,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[\"rust\"]","CreatedTime":"2026-01-06T10:00:00","UpdatedTime":"2026-01-06T10:00:00"}),
                json!({"NoteId":"n3","Title":"GPUI notes","NotebookId":"nb2","Usn":6,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[\"rust\",\"gpui\"]","CreatedTime":"2026-01-07T10:00:00","UpdatedTime":"2026-01-07T10:00:00"}),
                json!({"NoteId":"n4","Title":"Markdown 渲染示例","NotebookId":"nb2a","Usn":13,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[\"demo\"]","CreatedTime":"2026-01-10T10:00:00","UpdatedTime":"2026-01-10T10:00:00"}),
                json!({"NoteId":"n5","Title":"Mermaid 图示例","NotebookId":"nb2a","Usn":14,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[\"demo\"]","CreatedTime":"2026-01-11T10:00:00","UpdatedTime":"2026-01-11T10:00:00"}),
                // TinyMCE 时代的富文本笔记（ADR-0001：只读渲染）
                json!({"NoteId":"n10","Title":"2018 归档","NotebookId":"nb1","Usn":15,"IsDeleted":false,"IsTrash":false,"IsMarkdown":false,"Tags":"[]","CreatedTime":"2026-01-12T10:00:00","UpdatedTime":"2026-01-12T10:00:00"}),
                json!({"NoteId":"n11","Title":"旅行清单","NotebookId":"nb3","Usn":16,"IsDeleted":false,"IsTrash":false,"IsMarkdown":false,"Tags":"[]","CreatedTime":"2026-01-13T10:00:00","UpdatedTime":"2026-01-13T10:00:00"}),
                json!({"NoteId":"n12","Title":"图片笔记","NotebookId":"nb3","Usn":17,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-14T10:00:00","UpdatedTime":"2026-01-14T10:00:00"}),
                json!({"NoteId":"n20","Title":"启动增量新笔记","NotebookId":"nb1","Usn":20,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-15T10:00:00","UpdatedTime":"2026-01-15T10:00:00"}),
                // 溢出走查（20 条上限 + 2 入口行 > 一屏）：逼出滚动/Tab 栏布局回归
                json!({"NoteId":"n40","Title":"填料 01","NotebookId":"nb1","Usn":18,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-16T10:00:00","UpdatedTime":"2026-01-16T10:00:00"}),
                json!({"NoteId":"n41","Title":"填料 02","NotebookId":"nb1","Usn":18,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-16T10:00:00","UpdatedTime":"2026-01-16T10:01:00"}),
                json!({"NoteId":"n42","Title":"填料 03","NotebookId":"nb1","Usn":18,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-16T10:00:00","UpdatedTime":"2026-01-16T10:02:00"}),
                json!({"NoteId":"n43","Title":"填料 04","NotebookId":"nb1","Usn":18,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-16T10:00:00","UpdatedTime":"2026-01-16T10:03:00"}),
                json!({"NoteId":"n44","Title":"填料 05","NotebookId":"nb1","Usn":18,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-16T10:00:00","UpdatedTime":"2026-01-16T10:04:00"}),
                json!({"NoteId":"n45","Title":"填料 06","NotebookId":"nb1","Usn":18,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-16T10:00:00","UpdatedTime":"2026-01-16T10:05:00"}),
                json!({"NoteId":"n46","Title":"填料 07","NotebookId":"nb1","Usn":18,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-16T10:00:00","UpdatedTime":"2026-01-16T10:06:00"}),
                json!({"NoteId":"n47","Title":"填料 08","NotebookId":"nb1","Usn":18,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-16T10:00:00","UpdatedTime":"2026-01-16T10:07:00"}),
                json!({"NoteId":"n48","Title":"填料 09","NotebookId":"nb1","Usn":18,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-16T10:00:00","UpdatedTime":"2026-01-16T10:08:00"}),
                json!({"NoteId":"n49","Title":"填料 10","NotebookId":"nb1","Usn":18,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-16T10:00:00","UpdatedTime":"2026-01-16T10:09:00"}),
                json!({"NoteId":"n50","Title":"填料 11","NotebookId":"nb1","Usn":18,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-16T10:00:00","UpdatedTime":"2026-01-16T10:10:00"}),
                json!({"NoteId":"n51","Title":"填料 12","NotebookId":"nb1","Usn":18,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-16T10:00:00","UpdatedTime":"2026-01-16T10:11:00"}),
                json!({"NoteId":"n52","Title":"填料 13","NotebookId":"nb1","Usn":18,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-16T10:00:00","UpdatedTime":"2026-01-16T10:12:00"}),
                json!({"NoteId":"n53","Title":"填料 14","NotebookId":"nb1","Usn":18,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-16T10:00:00","UpdatedTime":"2026-01-16T10:13:00"}),
                json!({"NoteId":"n54","Title":"填料 15","NotebookId":"nb1","Usn":18,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-16T10:00:00","UpdatedTime":"2026-01-16T10:14:00"}),
                json!({"NoteId":"n55","Title":"填料 16","NotebookId":"nb1","Usn":18,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-16T10:00:00","UpdatedTime":"2026-01-16T10:15:00"}),
                json!({"NoteId":"n56","Title":"填料 17","NotebookId":"nb1","Usn":18,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-16T10:00:00","UpdatedTime":"2026-01-16T10:16:00"}),
                json!({"NoteId":"n57","Title":"填料 18","NotebookId":"nb1","Usn":18,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-16T10:00:00","UpdatedTime":"2026-01-16T10:17:00"}),
            ]);
            let mut contents = serde_json::Map::new();
            contents.insert("n1".into(), json!("# Welcome\n\n你好，leanote 移动端。"));
            contents.insert(
                "n4".into(),
                json!("# Markdown 渲染示例\n\n**加粗** *斜体* `code` 与 [链接](https://leanote.com)。\n\n- 列表项一\n- 列表项二\n\n> 引用块\n\n| 甲 | 乙 |\n| --- | --- |\n| 1 | 2 |"),
            );
            contents.insert(
                "n5".into(),
                json!("# Mermaid\n\n```mermaid\ngraph TD\n  A[笔记] --> B{渲染}\n  B -->|md| C[TextView]\n  B -->|mermaid| D[SVG]\n```"),
            );
            contents.insert("n10".into(), json!("<h2>2018 归档</h2><p>富文本笔记，<span style=\"color: #e03e3d;\">彩色文字</span>与<strong>加粗</strong>共存。</p><ul><li>第一季度<ul><li>一月</li></ul></li><li>第二季度</li></ul><table><thead><tr><th>事件</th><th>月份</th></tr></thead></table>"));
            contents.insert("n11".into(), json!("<h3>旅行清单</h3><p><em>北海道</em>，<u>冬季</u>出发。</p><ol><li>机票</li><li>酒店</li></ol>"));
            contents.insert(
                "n12".into(),
                json!("# 图片笔记\n\n![dot](leanote://file/getImage?fileId=srvimg_demo)"),
            );
            server.set_contents(contents);
            // 1x1 红点 PNG（桌面 seed 同款）
            server.set_images([(
                "srvimg_demo".to_string(),
                ("dot.png".to_string(), "image/png".to_string(), vec![
                    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d,
                    0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
                    0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xde, 0x00, 0x00, 0x00,
                    0x0c, 0x49, 0x44, 0x41, 0x54, 0x08, 0xd7, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
                    0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xdd, 0x8d, 0xb0, 0x00, 0x00, 0x00,
                    0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
                ]),
            )]);
            let _ = tx.send(server.url());
            std::future::pending::<()>().await; // keep rt (and server) alive
        });
    });
    rx
}
