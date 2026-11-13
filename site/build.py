#!/usr/bin/env python3
"""Builds the website (landing page + live demo) into site/dist. Usage: python3 site/build.py [https://site.url]
The demo is the real web UI (web/) with site/demo.js faking the API in the browser, over a snapshot in site/demo-data/."""
import re, shutil, sys, tomllib
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent; SITE = ROOT / 'site'; OUT = SITE / 'dist'
URL = (sys.argv[1] if len(sys.argv) > 1 else 'https://getpapiro.pages.dev').rstrip('/')
VERSION = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
shutil.rmtree(OUT, ignore_errors=True); OUT.mkdir()
def put(src, dst):
    (OUT / dst).parent.mkdir(parents=True, exist_ok=True); shutil.copy2(src, OUT / dst)
# landing
(OUT / 'index.html').write_text((SITE / 'index.html').read_text().replace('%SITE%', URL).replace('%VERSION%', VERSION))
for f in ('logo.svg', 'chart-footprint.svg', 'chart-speed.svg', 'desktop.jpg', 'mobile.jpg'): put(ROOT / 'docs/img' / f, f'img/{f}')
put(ROOT / 'web/og.png', 'img/og.png')
for f in (SITE / 'fonts').glob('*.woff2'): put(f, f'fonts/{f.name}')
# the app, as shipped in the binary
for f in ('app.js', 'app.css', 'preact.js', 'icon.svg', 'icon-192.png', 'icon-512.png', 'apple-touch-icon.png'): put(ROOT / 'web' / f, f'assets/{f}')
page = (ROOT / 'web/index.html').read_text().replace('%ORIGIN%', URL)
page = re.sub(r'<link rel="manifest"[^>]*>\n?', '', page).replace('<title>Papiro</title>', '<title>Papiro live demo</title>')
banner = ('<style>.demo-bar{position:fixed;inset:0 0 auto 0;height:30px;z-index:200;display:flex;align-items:center;justify-content:center;gap:10px;'
          'background:#ea580c;color:#1a0d04;font:600 13px/1 system-ui,sans-serif}.demo-bar a{color:inherit}'
          '.shell{margin-top:30px;height:calc(100dvh - 30px)!important}.side{top:30px!important}.offline{display:none}</style>')
bar = '<div class="demo-bar"><a href="/">Papiro</a> live demo · read-only · <a href="https://github.com/elpablazo/papiro">Run your own →</a></div>'
page = page.replace('</head>', banner + '\n<script src="/demo/demo.js"></script>\n</head>').replace('<div id="app"></div>', bar + '\n<div id="app"></div>')
(OUT / 'demo').mkdir(); (OUT / 'demo/index.html').write_text(page)
put(SITE / 'demo.js', 'demo/demo.js'); put(SITE / 'demo-data/data.json', 'demo/data.json'); put(SITE / 'demo-data/openapi.json', 'demo/openapi.json')
shutil.copytree(SITE / 'demo-data/api', OUT / 'api')
(OUT / '_headers').write_text('''/*
  X-Content-Type-Options: nosniff
  Referrer-Policy: strict-origin-when-cross-origin
/api/documents/:id/thumb
  Content-Type: image/jpeg
/api/documents/:id/pages/:n
  Content-Type: image/jpeg
/api/documents/:id/file
  Content-Type: application/pdf
/assets/*
  Cache-Control: public, max-age=3600
/img/*
  Cache-Control: public, max-age=86400
''')
# the app registers /sw.js; in the demo it does nothing (no offline cache, always the latest build)
(OUT / 'sw.js').write_text("self.addEventListener('install', () => self.skipWaiting());\n")
(OUT / 'robots.txt').write_text(f'User-agent: *\nAllow: /\nDisallow: /api/\nSitemap: {URL}/sitemap.xml\n')
(OUT / 'sitemap.xml').write_text(f'<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n<url><loc>{URL}/</loc></url>\n<url><loc>{URL}/demo/</loc></url>\n</urlset>\n')
print(f'{OUT} ({sum(1 for _ in OUT.rglob("*") if _.is_file())} files) for {URL}')
