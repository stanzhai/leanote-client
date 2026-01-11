#!/usr/bin/env node
'use strict';

const fs = require('fs');
const path = require('path');
const glob = require('glob');
const less = require('less');
const CleanCSS = require('clean-css');
const chokidar = require('chokidar');

const root = path.join(__dirname, '..');
const patterns = [
  path.join(root, 'public', 'themes', '**', '*.less'),
  path.join(root, 'public', 'css', '**', '*.less')
];

function compileLessFile(file) {
  return fs.promises.readFile(file, 'utf8')
    .then(content => less.render(content, { filename: path.resolve(file) }))
    .then(output => {
      const minified = new CleanCSS({ compatibility: 'ie8', inline: false }).minify(output.css);
      if (minified.errors && minified.errors.length) {
        console.error('CleanCSS errors', minified.errors);
      }
      const css = minified.styles || output.css;
      const outFile = file.replace(/\.less$/i, '.css');
      return fs.promises.writeFile(outFile, css, 'utf8')
        .then(() => console.log('Built:', path.relative(root, outFile)));
    })
    .catch(err => {
      console.error('Error building', file, err);
    });
}

async function buildAll() {
  const files = (await Promise.all(patterns.map(p => new Promise((res, rej) => glob(p, (err, matches) => err ? rej(err) : res(matches)))))).flat();
  await Promise.all(files.map(f => compileLessFile(f)));
}

const args = process.argv.slice(2);
const watch = args.includes('--watch') || args.includes('-w');

buildAll().then(() => {
  console.log('Initial less build complete');
  if (watch) {
    const watcher = chokidar.watch(patterns, { ignoreInitial: true });
    watcher.on('add', compileLessFile);
    watcher.on('change', compileLessFile);
    watcher.on('unlink', file => {
      const out = file.replace(/\.less$/i, '.css');
      fs.unlink(out, () => {});
      console.log('Removed:', path.relative(root, out));
    });
    console.log('Watching less files for changes...');
  }
}).catch(err => {
  console.error(err);
  process.exit(1);
});
