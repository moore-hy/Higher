# 生成一份最小的、真实可解析的单页 PDF（非敏感内容），用于 M4 的 PDF 路径烟测。
# 手工构造 + 精确 xref 偏移，避免引入任何额外依赖。
import io

LINES = [
    ("H1", "Higher O2 PDF Smoke Fixture"),
    ("H2", "Section One"),
    ("P",  "The mitochondrion is the powerhouse of the cell. It produces ATP"),
    ("P",  "through oxidative phosphorylation."),
    ("H2", "Section Two"),
    ("P",  "Photosynthesis converts light energy into chemical energy in"),
    ("P",  "chloroplasts, producing glucose and oxygen."),
]

SIZE = {"H1": 20, "H2": 15, "P": 11}
LEAD = {"H1": 30, "H2": 26, "P": 17}

parts, y = [], 720
for kind, text in LINES:
    esc = text.replace("\\", r"\\").replace("(", r"\(").replace(")", r"\)")
    parts.append("BT /F1 %d Tf 72 %d Td (%s) Tj ET" % (SIZE[kind], y, esc))
    y -= LEAD[kind]
content = "\n".join(parts).encode("latin-1")

objs = [
    b"<< /Type /Catalog /Pages 2 0 R >>",
    b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
    b"/Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
    b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    b"<< /Length " + str(len(content)).encode() + b" >>\nstream\n" + content + b"\nendstream",
]

out = bytearray(b"%PDF-1.4\n")
offsets = []
for i, body in enumerate(objs, start=1):
    offsets.append(len(out))
    out += str(i).encode() + b" 0 obj\n" + body + b"\nendobj\n"

xref_at = len(out)
out += b"xref\n0 " + str(len(objs) + 1).encode() + b"\n"
out += b"0000000000 65535 f \n"
for off in offsets:
    out += ("%010d 00000 n \n" % off).encode()
out += (b"trailer\n<< /Size " + str(len(objs) + 1).encode()
        + b" /Root 1 0 R >>\nstartxref\n" + str(xref_at).encode() + b"\n%%EOF\n")

with io.open(r"C:/Users/37653/Desktop/Higher/.higher/o2_fixture.pdf", "wb") as f:
    f.write(bytes(out))
print("wrote o2_fixture.pdf bytes=%d" % len(out))
