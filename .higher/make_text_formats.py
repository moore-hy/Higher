# 生成 csv / txt / ascii / html / htm 五份最小夹具，用于 M4 的纯文本路径烟测。
# 这些都是纯文本，无需 ZIP/OOXML —— stdlib 直接写文件，不引入任何额外依赖。

import csv as _csv

BASE = r"C:/Users/37653/Desktop/Higher/.higher/o2_fixture"

# 用 csv 模块写出，确保含逗号的事实字段被正确引用 —— 否则 docling 会报
# “列数不一致”并产空内容（那是夹具缺陷，不是 Higher 的行为）。
CSV_ROWS = [
    ["topic", "fact"],
    ["Mitochondrion", "The powerhouse of the cell, producing ATP through oxidative phosphorylation."],
    ["Photosynthesis", "Converts light energy into chemical energy in chloroplasts, producing glucose."],
]

import io

_buf = io.StringIO()
_w = _csv.writer(_buf, lineterminator="\n")
_w.writerows(CSV_ROWS)
CSV = _buf.getvalue()

TXT = """Higher O2 Text Smoke Fixture

Section One
The mitochondrion is the powerhouse of the cell. It produces ATP
through oxidative phosphorylation.

Section Two
Photosynthesis converts light energy into chemical energy in
chloroplasts, producing glucose and oxygen.
"""

HTML = """<!DOCTYPE html>
<html><head><title>Higher O2 HTML Smoke Fixture</title></head>
<body>
<h1>Higher O2 HTML Smoke Fixture</h1>
<h2>Section One</h2>
<p>The mitochondrion is the powerhouse of the cell. It produces ATP
through oxidative phosphorylation.</p>
<h2>Section Two</h2>
<p>Photosynthesis converts light energy into chemical energy in
chloroplasts, producing glucose and oxygen.</p>
</body></html>
"""

FILES = {
    ".csv": CSV,
    ".txt": TXT,
    ".ascii": TXT,
    ".html": HTML,
    ".htm": HTML,
}

import os

for ext, body in FILES.items():
    path = BASE + ext
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(body)
    print("wrote %s bytes=%d" % (os.path.basename(path), os.path.getsize(path)))
