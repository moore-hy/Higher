# 生成一份最小的、真实可解析的 XLSX（非敏感内容），用于 M4 的 XLSX 路径烟测。
# 与 DOCX / PPTX 同为 OOXML + ZIP，stdlib 的 zipfile 足够 —— 不引入任何额外依赖。
#
# 用 inline string（t="inlineStr"）省掉 sharedStrings.xml。
# 关系仍要挂在**拥有它的部件**上：worksheet 的关系在 xl/_rels/workbook.xml.rels。
import zipfile

NS = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
CT = "http://schemas.openxmlformats.org/package/2006/content-types"
RL = "http://schemas.openxmlformats.org/package/2006/relationships"
RT = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"

ROWS = [
    ["Topic", "Fact"],
    ["Mitochondrion", "The powerhouse of the cell, producing ATP through oxidative phosphorylation."],
    ["Photosynthesis", "Converts light energy into chemical energy in chloroplasts, producing glucose."],
]


def cell(ref, text):
    return '<c r="%s" t="inlineStr"><is><t>%s</t></is></c>' % (ref, text)


sheet_rows = "".join(
    '<row r="%d">%s</row>' % (i, "".join(cell("%s%d" % (chr(65 + j), i), v) for j, v in enumerate(row)))
    for i, row in enumerate(ROWS, start=1)
)

PARTS = {
    "[Content_Types].xml": """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="%s">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>""" % CT,

    "_rels/.rels": """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="%s">
<Relationship Id="rId1" Type="%s/officeDocument" Target="xl/workbook.xml"/>
</Relationships>""" % (RL, RT),

    "xl/workbook.xml": """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="%s" xmlns:r="%s">
<sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets>
</workbook>""" % (NS, R),

    "xl/_rels/workbook.xml.rels": """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="%s">
<Relationship Id="rId1" Type="%s/worksheet" Target="worksheets/sheet1.xml"/>
</Relationships>""" % (RL, RT),

    "xl/worksheets/sheet1.xml": """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="%s"><sheetData>%s</sheetData></worksheet>""" % (NS, sheet_rows),
}

out = r"C:/Users/37653/Desktop/Higher/.higher/o2_fixture.xlsx"
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    for name, body in PARTS.items():
        z.writestr(name, body)

import os
print("wrote o2_fixture.xlsx bytes=%d parts=%d" % (os.path.getsize(out), len(PARTS)))
