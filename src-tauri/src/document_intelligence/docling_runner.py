"""NIGHT SHIFT O2 · M4 — Higher 与 Docling 之间的**薄适配器**。

这不是 Docling 的源码，也不是一个解析器实现。它只做一件事：

    文件字节 → 调用成熟运行时 docling → 确定性 JSON（章节树 + chunk）

真正的解析（PDF / DOCX / PPTX / OCR / 版面分析）全部由 docling 完成。
Higher 只负责把 docling 的 `DoclingDocument` **投影**成自己的契约形状。

# 为什么是子进程 + JSON，而不是别的

Higher 没有内嵌 Python 运行时。成熟解析能力是一个 Python 包。
「子进程 + 一行 JSON 契约」是这个边界上**最薄**的实现：
- 运行时可以整体替换（升级 docling、换镜像、换机器）而不动 Higher 一行代码；
- 解析崩溃不会带走 Higher 进程；
- 契约是文本，可被独立验证。

# 为什么 ordinal 在这里生成

`Context Compiler` 的稳定排序（相关度 → doc → revision → section → chunk）
要求同一份材料 + 同一个 parser 版本必须得到同一串序号。
所以序号来自**文档自身的遍历顺序**，绝不来自时间、随机数或并发完成顺序。

# 退出码就是错误分类（与 Rust 侧 ParseFailure 一一对应）

    0  成功，stdout 是一份 JSON
    3  运行时不可用（docling 导入失败）      -> RuntimeUnavailable
    4  输入不被支持（后缀未知 / 空文件）      -> Unsupported
    5  解析过程报错                          -> Failed
"""

import json
import os
import sys

EXIT_OK = 0
EXIT_RUNTIME_UNAVAILABLE = 3
EXIT_UNSUPPORTED = 4
EXIT_FAILED = 5

# docling 官方支持且 Higher 会送去解析的后缀。
# 不在这里的后缀直接判 Unsupported —— 让运行时去猜只会得到更差的报错。
SUPPORTED_SUFFIXES = {
    ".pdf", ".docx", ".pptx", ".xlsx", ".html", ".htm", ".md", ".txt", ".asciidoc", ".csv",
}

# 表格格式（Higher 宣称为支持，但 docling 2.73.0 把它们 emit 成单个「table」item、
# 且**没有 text**）。上面的投影会丢掉它，于是「能解析却什么都学不到」——一份对
# Higher 没有价值的表格，若静默标成 Ready(0 chunks) 反而会让用户误以为导入成功。
# 所以这里把「解析成功但 0 个可学习 chunk」的表格格式按 UNSUPPORTED_INPUT 处理。
TABULAR_SUFFIXES = {".xlsx", ".csv"}


def _distribution_version(name):
    """已安装发行版的真实版本号；拿不到就返回 None（绝不编造）。

    注意 `docling` 2.73.0 不暴露 `__version__`，所以只能用 importlib.metadata。
    """
    try:
        from importlib.metadata import PackageNotFoundError, version

        try:
            return version(name)
        except PackageNotFoundError:
            return None
    except Exception:  # noqa: BLE001
        return None


def _fail(code, message):
    """把失败写到 stderr 并以对应退出码结束（stdout 保持干净）。"""
    sys.stderr.write(message)
    sys.exit(code)


def main():
    if len(sys.argv) != 2:
        _fail(EXIT_FAILED, "usage: docling_runner.py <path>")

    path = sys.argv[1]
    if not os.path.isfile(path):
        _fail(EXIT_FAILED, "input file not found: %s" % path)

    suffix = os.path.splitext(path)[1].lower()
    if suffix not in SUPPORTED_SUFFIXES:
        _fail(EXIT_UNSUPPORTED, "unsupported suffix: %s" % suffix)

    if os.path.getsize(path) == 0:
        _fail(EXIT_UNSUPPORTED, "empty input file")

    try:
        from docling.document_converter import DocumentConverter
        from docling.datamodel.base_models import InputFormat
    except Exception as exc:  # noqa: BLE001 - 任何导入失败都等价于「运行时不可用」
        _fail(EXIT_RUNTIME_UNAVAILABLE, "docling import failed: %r" % (exc,))

    # 版本必须走 importlib.metadata：docling 2.73.0 **没有** `__version__` 属性，
    # 用 getattr(docling, "__version__") 会永远拿到 None，于是审计信息里
    # document_revisions.parser_version 就成了一句谎话（或干脆是空的）。
    version = _distribution_version("docling")

    try:
        converter = DocumentConverter()
        if suffix == ".txt":
            # docling 根本没有「纯文本」InputFormat —— 一份 `.txt` 会被它判成
            # "format None does not match" 而 PARSER_FAILED。但 Markdown 后端能正确
            # 吃下散文（没有 `#` 的行就是正文 chunk），所以这里显式按 MD 解析，
            # 让「被 Higher 宣称为支持」的 .txt 不再悄悄失败。
            with open(path, "r", encoding="utf-8") as _fh:
                _text = _fh.read()
            result = converter.convert_string(_text, InputFormat.MD)
        else:
            result = converter.convert(path)
        doc = result.document
    except Exception as exc:  # noqa: BLE001 - 运行时确实在，但这份输入解析不了
        _fail(EXIT_FAILED, "docling convert failed: %r" % (exc,))

    try:
        payload = _project(doc, version)
    except Exception as exc:  # noqa: BLE001
        _fail(EXIT_FAILED, "projection failed: %r" % (exc,))

    # 表格格式的已知限制：docling 把表内容 emit 成「table」item 且无 text，投影后
    # 0 chunk。一份「能解析但学不到任何东西」的表格对 Higher 没有价值，静默标成
    # Ready(0 chunks) 只会误导用户以为导入成功。按 UNSUPPORTED_INPUT 处理，让调用方
    # 拿到明确的「这份输入学不到东西」。非表格格式的 0 chunk 不在此列（例如纯图 PDF
    # 是另一种语义，不在本决定的范围内）。
    if suffix in TABULAR_SUFFIXES and not payload["chunks"]:
        _fail(
            EXIT_UNSUPPORTED,
            "xlsx/csv parsed but yielded 0 learnable chunks: docling emits tabular "
            "content as a `table` item with no text, so there is nothing for Higher to "
            "learn from. Treated as UNSUPPORTED_INPUT.",
        )

    json.dump(payload, sys.stdout, ensure_ascii=False)
    sys.stdout.flush()
    sys.exit(EXIT_OK)


def _project(doc, version):
    """把 `DoclingDocument` 投影成 Higher 的契约。

    只做两件事：
    1. 用文档自身的遍历顺序给章节和 chunk 编号（确定性）；
    2. 把标题层级拍平成 `parent_index`（同一 sections 数组内的下标）。

    绝不在这里做语义判断、绝不生成摘要、绝不产出任何「学习结论」。
    """
    sections = []
    # label -> section 下标；用于把后续文本块挂到最近的标题下。
    heading_stack = []
    current_section = None

    chunks = []
    ordinal = 0

    for item in _iter_items(doc):
        label = _label_of(item)
        text = (getattr(item, "text", "") or "").strip()

        if label == "section_header" or label == "title":
            # 新章节：按自身标题层级决定父级。
            #
            # docling 会把文档标题标成 `title`（**没有** level），而把 H2 及以下
            # 标成 `section_header` 并从 level=1 开始 —— 也就是说它已经替我们
            # 压平了一层。把 `title` 当作 level 0 的章节，层级才是忠实的：
            # 文档标题成为根章节，H2 挂在它下面，于是**没有**任何 chunk 会
            # 落进「无章节」的孤儿状态，parent_context 也才有东西可用。
            level = 0 if label == "title" else int(getattr(item, "level", 1) or 1)
            while heading_stack and heading_stack[-1][0] >= level:
                heading_stack.pop()
            parent_index = heading_stack[-1][1] if heading_stack else None
            sections.append(
                {
                    "title": text or None,
                    "ordinal": len(sections),
                    "parent_index": parent_index,
                }
            )
            heading_stack.append((level, len(sections) - 1))
            current_section = len(sections) - 1
            continue

        if not text:
            continue

        chunks.append(
            {
                "ordinal": ordinal,
                "text": text,
                "section_index": current_section,
            }
        )
        ordinal += 1

    # 没有任何标题的文档仍然必须可用：给一个顶层章节兜底，
    # 否则所有 chunk 都会是「无章节」，邻接扩展就失去了依据。
    if not sections and chunks:
        sections.append({"title": None, "ordinal": 0, "parent_index": None})
        for c in chunks:
            c["section_index"] = 0

    return {
        "parser_name": "docling",
        "parser_version": version,
        "sections": sections,
        "chunks": chunks,
    }


def _iter_items(doc):
    """按文档顺序产出顶层 item（迭代式，避免深递归）。"""
    for item, _level in doc.iterate_items():
        yield item


def _label_of(item):
    label = getattr(item, "label", None)
    if label is None:
        return ""
    # docling 的 label 是枚举，取它的值；取不到就退化成字符串。
    return getattr(label, "value", str(label))


if __name__ == "__main__":
    main()
