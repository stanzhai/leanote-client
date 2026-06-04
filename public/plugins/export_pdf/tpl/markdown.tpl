<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta http-equiv="X-UA-Compatible" content="IE=edge">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{title}</title>
<style>
/* ==============================
   PDF Export — Markdown Notes
   Matches presentation.css style
   ============================== */

* {
  font-size: 16px;
}

body {
  margin: 0;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Noto Sans", "PingFang SC", "Microsoft YaHei", sans-serif;
  color: #1a1a1a;
  line-height: 1.7;
}

.note-container {
  width: 780px;
  margin: auto;
  padding: 20px 0;
}

#content-container {
  max-width: 100%;
}

/* --- Title --- */
#leanote-title {
  font-size: 1.8em;
  margin: 0 0 1.2em;
  font-weight: 700;
  color: #111;
  line-height: 1.3;
  letter-spacing: -0.02em;
  padding-bottom: 0.3em;
  border-bottom: 2px solid #e1e4e8;
}

/* --- Headings --- */
h1, h2, h3, h4, h5, h6 {
  margin-top: 1.8em;
  margin-bottom: 0.6em;
  font-weight: 600;
  line-height: 1.3;
  color: #111;
  letter-spacing: -0.01em;
}

h1 {
  font-size: 1.6em;
  padding-bottom: 0.3em;
  border-bottom: 1px solid #e1e4e8;
}

h2 {
  font-size: 1.35em;
  padding-bottom: 0.25em;
  border-bottom: 1px solid #e1e4e8;
}

h3 { font-size: 1.15em; }
h4 { font-size: 1em; }

/* --- Paragraphs --- */
p {
  margin: 0 0 1.2em;
}

/* --- Links --- */
a {
  color: #0969da;
  text-decoration: none;
}

/* --- Strong --- */
strong {
  font-weight: 600;
  color: #111;
}

/* --- Horizontal rule --- */
hr {
  margin: 2.5em 0;
  border: none;
  border-top: 1px solid #e1e4e8;
}

/* --- Images --- */
img {
  max-width: 100%;
  border-radius: 6px;
}

/* --- Lists --- */
ul, ol {
  padding-left: 2em;
  margin: 0 0 1.2em;
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
  margin: 0 0 1.2em;
  padding: 14px 20px;
  border-left: 4px solid #0969da;
  background: #f6f8fa;
  border-radius: 0 6px 6px 0;
  color: #57606a;
}

blockquote p {
  margin-bottom: 0.6em;
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
  font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", "Courier New", monospace;
}

code {
  padding: 2px 6px;
  font-size: 0.88em;
  color: #cf222e;
  background: #f6f8fa;
  border: 1px solid #e1e4e8;
  border-radius: 4px;
  white-space: normal;
  word-break: break-word;
}

pre {
  margin: 0 0 1.2em;
  padding: 18px;
  background: #f6f8fa;
  border: 1px solid #e1e4e8;
  border-radius: 6px;
  overflow-x: auto;
  font-size: 0.88em;
  line-height: 1.5;
  word-break: break-word;
}

pre code {
  padding: 0;
  font-size: inherit;
  color: inherit;
  background: transparent;
  border: none;
  border-radius: 0;
  white-space: pre;
  word-break: normal;
}

/* --- Tables --- */
table {
  width: 100%;
  margin: 0 0 1.2em;
  border-collapse: collapse;
  border-spacing: 0;
  font-size: 0.95em;
}

table th, table td {
  padding: 10px 14px;
  border: 1px solid #d8dee4;
  text-align: left;
  vertical-align: top;
}

table th {
  font-weight: 600;
  background: #f6f8fa;
  color: #111;
}

table tbody > tr:nth-child(even) > td,
table tbody > tr:nth-child(even) > th {
  background: #fafbfc;
}

/* Rich text editor table classes */
.mce-item-table, .mce-item-table td, .mce-item-table th, .mce-item-table caption {
  border: 1px solid #d8dee4;
  border-collapse: collapse;
  padding: 10px 14px;
}

/* --- Keyboard --- */
kbd {
  padding: 2px 6px;
  font-size: 0.85em;
  font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", "Courier New", monospace;
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
  font-size: 0.8em;
}

/* --- Diagrams --- */
.sequence-diagram, .flow-chart {
  text-align: center;
  margin-bottom: 1.2em;
}

.sequence-diagram text, .flow-chart text {
  font-size: 15px !important;
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
</head>

<body>

	<div class="note-container">
		<h1 class="title" id="leanote-title">{title}</h1>
		<div class="content-container html" id="content-container">
			<textarea id="leanote-content-markdown" style="display: none">{content}</textarea>
			<div class="content-html" id="leanote-content-html"></div>
		</div>
	</div>

<script src="leanote://public/libs/md2html/md2html.js"></script>
<script>
function init() {
	md2Html(document.getElementById('leanote-content-markdown').value, document.getElementById('leanote-content-html'), function(html) {
		// done
	});
}
init();
</script>
</body>
</html>
