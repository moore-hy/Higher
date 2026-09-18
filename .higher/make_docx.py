# 生成一份最小的、真实可解析的 DOCX（非敏感内容），用于 M4 的 DOCX 路径烟测。
# DOCX 本质是一包 OOXML + ZIP，stdlib 的 zipfile 足够 —— 不引入任何额外依赖。
import zipfile

PARTS = {
    "[Content_Types].xml": """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>""",

    # 包级关系只指向 document.xml —— **样式关系必须挂在 document.xml 上**
    # （word/_rels/document.xml.rels），不能挂在包根上。挂在包根时 python-docx
    # 看不到 styles 部件，于是每个段落的 pStyle 都解析失败、统统退回 Normal，
    # 标题就永远不会被认成 section_header。
    "_rels/.rels": """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>""",

    "word/_rels/document.xml.rels": """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>""",

    # 只用 Word 内置样式名：Title / Heading1 / Heading2。
    # docling 的 docx 后端靠段落样式名判定标题层级，所以样式名必须真实。
    "word/styles.xml": """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:style w:type="paragraph" w:styleId="Normal" w:default="1"><w:name w:val="Normal"/></w:style>
<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/></w:style>
<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/></w:style>
</w:styles>""",

    "word/document.xml": """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>
<w:p><w:pPr><w:pStyle w:val="Title"/></w:pPr><w:r><w:t>Higher O2 DOCX Smoke Fixture</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:t>Section One</w:t></w:r></w:p>
<w:p><w:r><w:t>The mitochondrion is the powerhouse of the cell. It produces ATP</w:t></w:r></w:p>
<w:p><w:r><w:t>through oxidative phosphorylation.</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:t>Section Two</w:t></w:r></w:p>
<w:p><w:r><w:t>Photosynthesis converts light energy into chemical energy in chloroplasts.</w:t></w:r></w:p>
</w:body>
</w:document>""",
}

import sys as _sys
out = _sys.argv[1] if len(_sys.argv) > 1 else r"C:/Users/37653/Desktop/Higher/.higher/o2_fixture.docx"
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    for name, body in PARTS.items():
        z.writestr(name, body)
