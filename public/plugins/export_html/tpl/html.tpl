<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta name="tool" content="leanote-desktop-app">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{title}</title>
<style>
/* ==============================
   HTML Export — Rich Text Notes
   Modern article style
   ============================== */

body {
  margin: 0;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Noto Sans", "PingFang SC", "Microsoft YaHei", "Hiragino Sans GB", sans-serif;
  color: #1a1a1a;
  line-height: 1.7;
  font-size: 16px;
  background: #fff;
}

.note-container {
  max-width: 780px;
  margin: auto;
  padding: 32px 24px 64px;
}

/* --- Title --- */
#leanote-title {
  font-size: 2em;
  margin: 0 0 1em;
  font-weight: 800;
  color: #0d0d0d;
  line-height: 1.25;
  letter-spacing: -0.03em;
  padding-bottom: 0.3em;
  border-bottom: 1px solid #d8dee4;
}

/* --- Headings --- */
h1, h2, h3, h4, h5, h6 {
  margin-top: 1.8em;
  margin-bottom: 0.5em;
  font-weight: 700;
  line-height: 1.3;
  color: #0d0d0d;
  letter-spacing: -0.02em;
}

h1 { font-size: 1.8em; }
h2 { font-size: 1.45em; }
h3 { font-size: 1.2em; }
h4 { font-size: 1em; font-weight: 600; }

/* --- Paragraphs --- */
p {
  margin: 0 0 1.4em;
}

/* --- Links --- */
a {
  color: #0969da;
  text-decoration: none;
}

a:hover {
  text-decoration: underline;
}

/* --- Strong --- */
strong, b {
  font-weight: 600;
  color: #0d0d0d;
}

/* --- Horizontal rule --- */
hr {
  margin: 2.5em 0;
  border: none;
  border-top: 1px solid #d8dee4;
}

/* --- Images --- */
img {
  max-width: 100%;
  border-radius: 8px;
  display: block;
  margin: 1.5em auto;
}

/* --- Lists --- */
ul, ol {
  padding-left: 1.8em;
  margin: 0 0 1.4em;
}

li {
  margin-bottom: 0.3em;
}

li > ul,
li > ol {
  margin-bottom: 0;
}

ul ul, ol ul, ul ol, ol ol {
  margin-bottom: 0;
}

/* --- Blockquote --- */
blockquote {
  margin: 0 0 1.4em;
  padding: 16px 24px;
  border-left: 4px solid #0969da;
  background: #f6f8fa;
  border-radius: 0 6px 6px 0;
  color: #57606a;
}

blockquote p {
  margin-bottom: 0.7em;
}

blockquote p:last-child {
  margin-bottom: 0;
}

blockquote ul:last-child,
blockquote ol:last-child {
  margin-bottom: 0;
}

/* --- Code --- */
pre, code {
  font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace;
}

code {
  padding: 2px 6px;
  font-size: 0.85em;
  color: #cf222e;
  background: #f6f8fa;
  border: 1px solid #d8dee4;
  border-radius: 4px;
  word-break: break-word;
}

pre {
  margin: 0 0 1.4em;
  padding: 20px;
  background: #f6f8fa;
  border: 1px solid #d8dee4;
  border-radius: 8px;
  overflow-x: auto;
  font-size: 0.85em;
  line-height: 1.55;
  word-break: normal;
}

pre code {
  padding: 0;
  font-size: inherit;
  color: inherit;
  white-space: pre;
  background: transparent;
  border: none;
  border-radius: 0;
}

/* --- Tables --- */
table {
  width: 100%;
  margin: 0 0 1.4em;
  border-collapse: collapse;
  border-spacing: 0;
  font-size: 0.95em;
}

table th, table td {
  padding: 10px 16px;
  border: 1px solid #d8dee4;
  text-align: left;
  vertical-align: top;
}

table th {
  font-weight: 600;
  background: #f6f8fa;
  color: #0d0d0d;
}

table tbody > tr:nth-child(even) > td,
table tbody > tr:nth-child(even) > th {
  background: #fafbfc;
}

.mce-item-table, .mce-item-table td, .mce-item-table th, .mce-item-table caption {
  border: 1px solid #d8dee4;
  border-collapse: collapse;
  padding: 10px 16px;
}

/* --- Keyboard --- */
kbd {
  padding: 2px 6px;
  font-size: 0.8em;
  font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace;
  color: #24292f;
  background: #f6f8fa;
  border: 1px solid #d8dee4;
  border-bottom-width: 2px;
  border-radius: 4px;
  box-shadow: 0 1px 0 #d8dee4;
  display: inline-block;
  margin: 0 0.1em;
  white-space: nowrap;
}

/* --- TOC --- */
.toc ul {
  list-style-type: none;
  margin-bottom: 15px;
}

/* --- Task list --- */
.m-todo-item {
  list-style: none;
}

/* --- Footnote --- */
.footnote {
  vertical-align: top;
  position: relative;
  top: -0.5em;
  font-size: 0.75em;
}

/* --- Diagrams --- */
.sequence-diagram, .flow-chart {
  text-align: center;
  margin-bottom: 1.4em;
}

.sequence-diagram text, .flow-chart text {
  font-size: 14px !important;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif !important;
}

.sequence-diagram [fill="#ffffff"], .flow-chart [fill="#ffffff"] {
  fill: #f6f6f6;
}

.sequence-diagram [stroke="#000000"], .flow-chart [stroke="#000000"] {
  stroke: #3f3f3f;
}

.sequence-diagram text[stroke="#000000"], .flow-chart text[stroke="#000000"] {
  stroke: none;
}

.sequence-diagram [fill="#000"], .flow-chart [fill="#000"],
.sequence-diagram [fill="#000000"], .flow-chart [fill="#000000"],
.sequence-diagram [fill="black"], .flow-chart [fill="black"] {
  fill: #3f3f3f;
}
</style>
<link href="../leanote-html.css" rel="stylesheet">
</head>

<body>

	<div class="note-container">
		<h1 class="title" id="leanote-title">{title}</h1>
		<div class="content-html" id="leanote-content">{content}</div>
	</div>

<script src="../leanote-html.js"></script>
</body>
</html>
