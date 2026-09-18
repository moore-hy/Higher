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
    ".pdf", ".docx", ".pptx", ".xlsx", ".html", ".htm", ".md", ".txt", ".ascii", ".csv",
}


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
        import docling
    except Exception as exc:  # noqa: BLE001 - 任何导入失败都等价于「运行时不可用」
        _fail(EXIT_RUNTIME_UNAVAILABLE, "docling import failed: %r" % (exc,))

    version = getattr(docling, "__version__", None)

    try:
        converter = DocumentConverter()
        result = converter.convert(path)
        doc = result.document
    except Exception as exc:  # noqa: BLE001 - 运行时确实在，但这份输入解析不了
        _fail(EXIT_FAILED, "docling convert failed: %r" % (exc,))

    try:
        payload = _project(doc, version)
    except Exception as exc:  # noqa: BLE001
        _fail(EXIT_FAILED, "projection failed: %r" % (exc,))

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

        if label == "section_header":
            # 新章节：按自身标题层级决定父级。
            level = int(getattr(item, "level", 1) or 1)
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
