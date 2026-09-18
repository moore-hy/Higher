# 生成一份最小的、真实可解析的 PPTX（非敏感内容），用于 M4 的 PPTX 路径烟测。
# PPTX 同样是 OOXML + ZIP，stdlib 的 zipfile 足够 —— 不引入任何额外依赖。
#
# 结构要点（与 DOCX 同一个教训）：关系必须挂在**拥有它的那个部件**上 ——
# presentation 的关系在 ppt/_rels/presentation.xml.rels，slide 的在
# ppt/slides/_rels/slide1.xml.rels，等等。挂在包根上解析器就找不到东西。
import zipfile

A = 'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"'
R = 'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"'
P = 'xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"'
CT = "http://schemas.openxmlformats.org/package/2006/content-types"
RL = "http://schemas.openxmlformats.org/package/2006/relationships"
RT = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"


def textbox(shape_id, name, x, y, cx, cy, lines):
    paras = "".join(
        "<a:p><a:r><a:rPr lang=\"en-US\"/><a:t>%s</a:t></a:r></a:p>" % t for t in lines
    )
    return """<p:sp>
<p:nvSpPr><p:cNvPr id="%d" name="%s"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr>
<p:spPr><a:xfrm><a:off x="%d" y="%d"/><a:ext cx="%d" cy="%d"/></a:xfrm>
<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
<p:txBody><a:bodyPr/><a:lstStyle/>%s</p:txBody>
</p:sp>""" % (shape_id, name, x, y, cx, cy, paras)


SPTREE_OPEN = """<p:cSld><p:spTree>
<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
<p:grpSpPr/>"""
SPTREE_CLOSE = """</p:spTree></p:cSld>"""

PARTS = {
    "[Content_Types].xml": """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="%s">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
<Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/>
<Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>
<Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>
</Types>""" % CT,

    "_rels/.rels": """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="%s">
<Relationship Id="rId1" Type="%s/officeDocument" Target="ppt/presentation.xml"/>
</Relationships>""" % (RL, RT),

    "ppt/presentation.xml": """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation %s %s %s>
<p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>
<p:sldIdLst><p:sldId id="256" r:id="rId2"/></p:sldIdLst>
<p:sldSz cx="12192000" cy="6858000"/><p:notesSz cx="6858000" cy="9144000"/>
</p:presentation>""" % (A, R, P),

    "ppt/_rels/presentation.xml.rels": """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="%s">
<Relationship Id="rId1" Type="%s/slideMaster" Target="slideMasters/slideMaster1.xml"/>
<Relationship Id="rId2" Type="%s/slide" Target="slides/slide1.xml"/>
</Relationships>""" % (RL, RT, RT),

    "ppt/slideMasters/slideMaster1.xml": """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldMaster %s %s %s>%s%s
<p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2"
 accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>
<p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst>
</p:sldMaster>""" % (A, R, P, SPTREE_OPEN, SPTREE_CLOSE),

    "ppt/slideMasters/_rels/slideMaster1.xml.rels": """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="%s">
<Relationship Id="rId1" Type="%s/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
</Relationships>""" % (RL, RT),

    "ppt/slideLayouts/slideLayout1.xml": """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout %s %s %s type="blank" preserve="1">%s%s
<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>
</p:sldLayout>""" % (A, R, P, SPTREE_OPEN, SPTREE_CLOSE),

    "ppt/slideLayouts/_rels/slideLayout1.xml.rels": """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="%s">
<Relationship Id="rId1" Type="%s/slideMaster" Target="../slideMasters/slideMaster1.xml"/>
</Relationships>""" % (RL, RT),

    "ppt/slides/slide1.xml": """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld %s %s %s>%s%s%s%s
<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>
</p:sld>""" % (
        A, R, P, SPTREE_OPEN,
        textbox(2, "Title 1", 838200, 365125, 10515600, 1325563,
                ["Higher O2 PPTX Smoke Fixture"]),
        textbox(3, "Content 1", 838200, 1825625, 10515600, 4351338,
                ["Section One",
                 "The mitochondrion is the powerhouse of the cell. It produces ATP",
                 "Section Two",
                 "Photosynthesis converts light energy into chemical energy in chloroplasts"]),
        SPTREE_CLOSE,
    ),

    "ppt/slides/_rels/slide1.xml.rels": """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="%s">
<Relationship Id="rId1" Type="%s/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
</Relationships>""" % (RL, RT),
}

out = r"C:/Users/37653/Desktop/Higher/.higher/o2_fixture.pptx"
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    for name, body in PARTS.items():
        z.writestr(name, body)

import os
print("wrote o2_fixture.pptx bytes=%d parts=%d" % (os.path.getsize(out), len(PARTS)))
