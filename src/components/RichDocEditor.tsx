import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  EditorContent,
  NodeViewWrapper,
  NodeViewContent,
  ReactNodeViewRenderer,
  useEditor,
} from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import CodeBlock from "@tiptap/extension-code-block";
import { Node, mergeAttributes } from "@tiptap/core";
import type { JSONContent } from "@tiptap/react";
import {
  addAttachmentFromBase64,
  addLearningAttachment,
  readAttachmentImage,
  saveDrawingAttachment,
} from "../api";
import type { AttachmentImageData } from "../types";
import { blobToBase64, fileToBase64 } from "../utils";
import DrawModal from "./DrawModal";

// ---------- CustomCodeBlock（DEV-0050 §8：正规 NodeView，替代 DOM 注入装饰层） ----------
// 结构：NodeViewWrapper > [工具栏(React)] + <pre><NodeViewContent/></pre>
// 代码文字由 ProseMirror 管理；复制按钮是 React 临时 UI state（不入 Tiptap JSON）。
// 禁止任何 appendChild/querySelectorAll/MutationObserver 修改 ProseMirror DOM（BUG-01 根因）。
// extend 官方 CodeBlock：保留全部 schema/命令/快捷键（toggleCodeBlock 等），只替换 NodeView。
const CustomCodeBlock = CodeBlock.extend({
  addNodeView() {
    return ReactNodeViewRenderer(CodeBlockView);
  },
});

function CodeBlockView({ node, deleteNode, selected }: { node: any; deleteNode: () => void; selected: boolean }) {
  const [copied, setCopied] = useState(false);
  const onCopy = useCallback(async () => {
    // 只复制代码正文（node.textContent = ProseMirror 管理的纯文本，无按钮文字）
    const text = node.content && typeof node.textContent === "string" ? node.textContent : "";
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      /* 剪贴板不可用时静默 */
    }
  }, [node]);
  return (
    <NodeViewWrapper className={"hdoc__codeblock" + (selected ? " hdoc__codeblock--sel" : "")}>
      <div className="hdoc__codebar" contentEditable={false}>
        <span className="hdoc__codebar-label">代码 / 命令</span>
        <div className="hdoc__codebar-actions">
          <button className="hdoc__code-copy" onClick={() => void onCopy()}>
            {copied ? "已复制" : "复制"}
          </button>
          <button className="hdoc__code-remove" title="删除此代码块" onClick={deleteNode}>
            删除
          </button>
        </div>
      </div>
      <pre>
        <NodeViewContent className="hdoc__codecontent" />
      </pre>
    </NodeViewWrapper>
  );
}

/**
 * Learning Editor V3（DEV-0049 / Fix 01-A）：Tiptap Word-like 连续文档编辑器。
 *
 * - 文字 / Heading(H2/H3) / 列表 / CodeBlock(带复制) / 图片 / 视频 / 画图 共处一个文档流
 * - 媒体只存 attachment_id 引用（higherImage/higherVideo Node attrs），不存 base64/绝对路径
 * - 加载兼容：note_document_json 优先；NULL 时按 note 旧 v2 JSON 或纯文本构造文档（§3.4）
 * - 输出：onChange(json, plainText) —— 外层 debounce 经 update_session_document 原子保存
 * - Ctrl+V 截图 / 拖入图片视频 / dialog 上传 / 画图 → 全部插入当前光标位置
 * - readOnly 用于 End Sheet 后的归档视图
 */

// ---------- 媒体加载缓存（模块级，跨重开复用） ----------
const imgCache = new Map<number, AttachmentImageData>();
async function loadAttachment(id: number): Promise<AttachmentImageData | null> {
  if (imgCache.has(id)) return imgCache.get(id)!;
  try {
    const d = await readAttachmentImage(id);
    imgCache.set(id, d);
    return d;
  } catch {
    return null;
  }
}

// ---------- higherImage Node ----------
const HigherImage = Node.create({
  name: "higherImage",
  group: "block",
  atom: true,
  draggable: true,
  addAttributes() {
    return {
      attachmentId: { default: null as number | null },
      fileName: { default: "" },
      caption: { default: "" },
    };
  },
  parseHTML() {
    return [{ tag: "higher-image" }];
  },
  renderHTML({ HTMLAttributes }) {
    return ["higher-image", mergeAttributes(HTMLAttributes)];
  },
  addNodeView() {
    return ReactNodeViewRenderer(ImageNodeView);
  },
});

function ImageNodeView({ node, deleteNode, selected }: { node: any; deleteNode: () => void; selected: boolean }) {
  const attId: number | null = node.attrs.attachmentId;
  const fileName: string = node.attrs.fileName ?? "";
  const [data, setData] = useState<AttachmentImageData | null>(null);
  const [full, setFull] = useState(false);
  useEffect(() => {
    if (attId != null) void loadAttachment(attId).then(setData);
  }, [attId]);
  return (
    <NodeViewWrapper className={"hdoc__media" + (selected ? " hdoc__media--sel" : "")}>
      {data ? (
        <img
          className="hdoc__img"
          src={`data:${data.mime_type};base64,${data.base64}`}
          alt={fileName}
          onClick={() => setFull(true)}
        />
      ) : (
        <div className="hdoc__loading">图片加载中…</div>
      )}
      {fileName && <div className="hdoc__caption">{fileName}</div>}
      <button className="hdoc__remove" title="从正文移除（附件保留）" onClick={deleteNode}>
        移除
      </button>
      {full && data && (
        <div className="modal-overlay" onClick={() => setFull(false)}>
          <div className="att-fullview" onClick={(e) => e.stopPropagation()}>
            <img src={`data:${data.mime_type};base64,${data.base64}`} alt="原图" />
            <div className="btn-row" style={{ justifyContent: "center", marginTop: 10 }}>
              <button className="btn btn--small" onClick={() => setFull(false)}>关闭</button>
            </div>
          </div>
        </div>
      )}
    </NodeViewWrapper>
  );
}

// ---------- higherVideo Node ----------
const HigherVideo = Node.create({
  name: "higherVideo",
  group: "block",
  atom: true,
  draggable: true,
  addAttributes() {
    return {
      attachmentId: { default: null as number | null },
      fileName: { default: "" },
      caption: { default: "" },
    };
  },
  parseHTML() {
    return [{ tag: "higher-video" }];
  },
  renderHTML({ HTMLAttributes }) {
    return ["higher-video", mergeAttributes(HTMLAttributes)];
  },
  addNodeView() {
    return ReactNodeViewRenderer(VideoNodeView);
  },
});

function VideoNodeView({ node, deleteNode }: { node: any; deleteNode: () => void }) {
  const attId: number | null = node.attrs.attachmentId;
  const fileName: string = node.attrs.fileName ?? "";
  const [data, setData] = useState<AttachmentImageData | null>(null);
  useEffect(() => {
    if (attId != null) void loadAttachment(attId).then(setData);
  }, [attId]);
  return (
    <NodeViewWrapper className="hdoc__media">
      {data ? (
        <video
          className="hdoc__video"
          controls
          preload="metadata"
          src={`data:${data.mime_type};base64,${data.base64}`}
        />
      ) : (
        <div className="hdoc__loading">视频加载中…</div>
      )}
      {fileName && <div className="hdoc__caption">{fileName}</div>}
      <button className="hdoc__remove" title="从正文移除（附件保留）" onClick={deleteNode}>
        移除
      </button>
    </NodeViewWrapper>
  );
}

// ---------- §3.4 旧 note → Tiptap 文档 兼容构造 ----------
type LegacyBlock = { t: string; c?: string; a?: number; n?: string };

export function noteToDocument(note: string | null | undefined): JSONContent {
  // 空 → 空文档
  if (!note || !note.trim()) return { type: "doc", content: [{ type: "paragraph" }] };
  const s = note.trim();
  if (!s.startsWith("{")) {
    // 纯文本：每行一个段落（保留换行结构）
    const lines = s.split(/\r?\n/);
    return {
      type: "doc",
      content: lines.map((l) =>
        l.trim() === ""
          ? { type: "paragraph" }
          : { type: "paragraph", content: l ? [{ type: "text", text: l }] : undefined }
      ),
    };
  }
  try {
    const parsed = JSON.parse(s);
    // 已经是 Tiptap doc
    if (parsed?.type === "doc" && Array.isArray(parsed.content)) return parsed as JSONContent;
    // 旧 v2 blocks → 文档流
    if (Array.isArray(parsed?.blocks)) {
      const content: JSONContent[] = [];
      for (const b of parsed.blocks as LegacyBlock[]) {
        if (b.t === "text" && b.c) {
          for (const line of b.c.split(/\r?\n/)) {
            content.push(
              line === ""
                ? { type: "paragraph" }
                : { type: "paragraph", content: [{ type: "text", text: line }] }
            );
          }
        } else if (b.t === "video") {
          content.push({ type: "higherVideo", attrs: { attachmentId: b.a ?? null, fileName: b.n ?? "", caption: "" } });
        } else if (b.t === "image" || b.t === "drawing") {
          content.push({ type: "higherImage", attrs: { attachmentId: b.a ?? null, fileName: b.n ?? "", caption: "" } });
        }
      }
      if (content.length === 0) content.push({ type: "paragraph" });
      return { type: "doc", content };
    }
  } catch {
    /* 回退文本 */
  }
  return { type: "doc", content: [{ type: "paragraph", content: [{ type: "text", text: s }] }] };
}

/** §9 纯文本投影：正文/CodeBlock 原文；媒体 → [图片: 文件名] / [视频: 文件名]。 */
export function documentToPlainText(doc: JSONContent | null): string {
  if (!doc) return "";
  const parts: string[] = [];
  const walk = (node: JSONContent) => {
    switch (node.type) {
      case "text":
        parts.push(node.text ?? "");
        break;
      case "hardBreak":
        parts.push("\n");
        break;
      case "codeBlock":
        parts.push((node.content ?? []).map((c) => c.text ?? "").join(""));
        break;
      case "higherImage": {
        const n = node.attrs?.fileName || "图片";
        parts.push(`[图片: ${n}]`);
        break;
      }
      case "higherVideo": {
        const n = node.attrs?.fileName || "视频";
        parts.push(`[视频: ${n}]`);
        break;
      }
      case "paragraph":
      case "heading":
      case "listItem": {
        const inner: string[] = [];
        for (const c of node.content ?? []) {
          const before = parts.length;
          walk(c);
          if (parts.length > before) inner.push(parts.pop()!);
        }
        let line = inner.join("");
        if (node.type === "listItem") line = "- " + line;
        if (node.type === "heading") line = "\n" + line;
        parts.push(line);
        break;
      }
      case "bulletList":
      case "orderedList":
      case "blockquote": {
        for (const c of node.content ?? []) walk(c);
        break;
      }
      case "doc": {
        const lines: string[] = [];
        for (const c of node.content ?? []) {
          const before = parts.length;
          walk(c);
          if (parts.length > before) lines.push(parts.pop()!);
        }
        parts.push(lines.join("\n"));
        break;
      }
      default: {
        for (const c of node.content ?? []) walk(c);
      }
    }
  };
  walk(doc);
  return parts.join("").replace(/\n{3,}/g, "\n\n").trim();
}

// ---------- 主组件 ----------
export default function RichDocEditor({
  profileId,
  learningItemId,
  sessionId,
  initialDocument,
  initialLegacyNote,
  onChange,
  readOnly = false,
}: {
  profileId: number;
  learningItemId: number | null;
  sessionId: number | null;
  /** note_document_json（可能为 null = 旧 Session） */
  initialDocument: JSONContent | null;
  /** 旧 note（initialDocument 为 null 时用于 §3.4 兼容构造） */
  initialLegacyNote: string | null;
  /** 文档变化：外部 debounce 保存（json + 纯文本投影） */
  onChange: (doc: JSONContent, plainText: string) => void;
  readOnly?: boolean;
}) {
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [showDraw, setShowDraw] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  /** 外部删除附件后要求移除正文引用（attachmentId → 同步删除 Node） */
  const [detachIds, setDetachIds] = useState<number[]>([]);
  const detachRef = useRef<number[]>([]);

  const editor = useEditor(
    {
      // codeBlock 用本文件 CustomCodeBlock NodeView 覆盖 StarterKit 内置版本
      extensions: [
        StarterKit.configure({
          heading: { levels: [2, 3] },
          codeBlock: false,
        }),
        CustomCodeBlock,
        HigherImage,
        HigherVideo,
      ],
      content: initialDocument ?? noteToDocument(initialLegacyNote),
      editable: !readOnly,
      editorProps: {
        attributes: { class: "hdoc__canvas", spellcheck: "false" },
      },
      onUpdate: ({ editor: e }) => {
        onChange(e.getJSON(), documentToPlainText(e.getJSON()));
      },
    },
    []
  );

  useEffect(() => {
    if (editor && readOnly !== !editor.isEditable) editor.setEditable(!readOnly);
  }, [editor, readOnly]);

  /** 附件被外部（附件区）删除 → 移除正文所有引用 Node（§8 同步移除）。
   * BUG-02 根因修复：仅在 detachIds 非空时消费并清空；空数组直接 return，
   * 禁止每次渲染 setState 新数组引用（曾导致 Maximum update depth → 全树冻结）。 */
  useEffect(() => {
    if (!editor || detachIds.length === 0) return;
    for (const id of detachIds) {
      const { state, view } = editor;
      state.doc.descendants((node, p) => {
        if (
          (node.type.name === "higherImage" || node.type.name === "higherVideo") &&
          node.attrs.attachmentId === id
        ) {
          view.dispatch(state.tr.delete(p, p + node.nodeSize));
        }
      });
    }
    setDetachIds([]);
  }, [editor, detachIds]);

  /** 插入媒体（当前选区位置；§5/§6/§7 统一入口） */
  const insertMedia = useCallback(
    (kind: "image" | "video", attId: number, fileName: string) => {
      if (!editor) return;
      editor
        .chain()
        .focus()
        .insertContent({
          type: kind === "video" ? "higherVideo" : "higherImage",
          attrs: { attachmentId: attId, fileName, caption: "" },
        })
        .run();
    },
    [editor]
  );

  /** 附件保存链（粘贴 base64 / 上传复制），成功后插入 */
  const saveAndInsert = useCallback(
    async (args: { kind: "image" | "video"; fileName: string; mime: string | null; base64?: string; sourcePath?: string }) => {
      setBusy(`正在插入 ${args.fileName}…`);
      try {
        const att =
          args.base64 != null
            ? await addAttachmentFromBase64({
                profileId,
                learningItemId,
                sessionId,
                attachmentType: args.kind,
                fileName: args.fileName,
                mimeType: args.mime,
                dataBase64: args.base64,
              })
            : await addLearningAttachment({
                profileId,
                learningItemId,
                sessionId,
                attachmentType: args.kind,
                sourcePath: args.sourcePath!,
                caption: "",
              });
        insertMedia(args.kind, att.id, att.file_name);
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy("");
      }
    },
    [profileId, learningItemId, sessionId, insertMedia]
  );

  // ---------- Ctrl+V 剪贴板图片（§5.1 A；DOM 事件即可插入当前光标） ----------
  useEffect(() => {
    if (!editor || readOnly) return;
    const dom = editor.view.dom;
    const onPaste = (ev: ClipboardEvent) => {
      const items = ev.clipboardData?.items;
      if (!items) return;
      for (const item of items) {
        if (item.type.startsWith("image/")) {
          ev.preventDefault();
          const blob = item.getAsFile();
          if (!blob) continue;
          void (async () => {
            const b64 = await blobToBase64(blob);
            const ext = (item.type.split("/")[1] || "png").replace("jpeg", "jpg");
            await saveAndInsert({ kind: "image", fileName: `粘贴图片.${ext}`, mime: item.type, base64: b64 });
          })();
          return;
        }
      }
    };
    dom.addEventListener("paste", onPaste);
    return () => dom.removeEventListener("paste", onPaste);
  }, [editor, readOnly, saveAndInsert]);

  // ---------- 拖放图片/视频（§5.1 B） ----------
  useEffect(() => {
    if (!editor || readOnly) return;
    const dom = editor.view.dom;
    const onDragOver = (e: DragEvent) => {
      e.preventDefault();
      setDragOver(true);
    };
    const onDragLeave = () => setDragOver(false);
    const onDrop = (e: DragEvent) => {
      e.preventDefault();
      setDragOver(false);
      const files = Array.from(e.dataTransfer?.files ?? []);
      for (const f of files) {
        const isImg = /\.(png|jpe?g|webp|gif|bmp)$/i.test(f.name);
        const isVid = /\.(mp4|webm|mov|mkv)$/i.test(f.name);
        if (!isImg && !isVid) continue;
        void fileToBase64(f).then((b64) =>
          saveAndInsert({ kind: isVid ? "video" : "image", fileName: f.name, mime: f.type || null, base64: b64 })
        );
      }
    };
    dom.addEventListener("dragover", onDragOver);
    dom.addEventListener("dragleave", onDragLeave);
    dom.addEventListener("drop", onDrop);
    return () => {
      dom.removeEventListener("dragover", onDragOver);
      dom.removeEventListener("dragleave", onDragLeave);
      dom.removeEventListener("drop", onDrop);
    };
  }, [editor, readOnly, saveAndInsert]);

  // ---------- 工具栏动作 ----------
  const uploadImage = useCallback(async () => {
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: "图片", extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp"] }],
    });
    if (!picked) return;
    await saveAndInsert({ kind: "image", fileName: String(picked), mime: null, sourcePath: String(picked) });
  }, [saveAndInsert]);

  const uploadVideo = useCallback(async () => {
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: "视频", extensions: ["mp4", "webm", "mov", "mkv"] }],
    });
    if (!picked) return;
    await saveAndInsert({ kind: "video", fileName: String(picked), mime: null, sourcePath: String(picked) });
  }, [saveAndInsert]);

  /** 附件区"插入正文"入口（§8：历史附件插入当前光标） */
  const insertExisting = useCallback(
    (attId: number, kind: "image" | "video", fileName: string) => {
      insertMedia(kind, attId, fileName);
    },
    [insertMedia]
  );

  /** 暴露给外层：附件删除后同步移除正文引用（一次一批，消费即清） */
  const detachAttachment = useCallback((attId: number) => {
    detachRef.current = [...detachRef.current, attId];
    setDetachIds([...detachRef.current]);
    detachRef.current = [];
  }, []);

  // 外部可通过 ref 使用（LearningWorkspace 用 state 转发即可，简化为 window 事件）
  useEffect(() => {
    const h = (ev: Event) => {
      const detail = (ev as CustomEvent<{ id: number; kind: "image" | "video"; name: string }>).detail;
      if (detail?.id != null && detail.kind) insertExisting(detail.id, detail.kind, detail.name ?? "");
    };
    window.addEventListener("higher:insert-attachment", h);
    const d = (ev: Event) => {
      const detail = (ev as CustomEvent<{ id: number }>).detail;
      if (detail?.id != null) detachAttachment(detail.id);
    };
    window.addEventListener("higher:detach-attachment", d);
    return () => {
      window.removeEventListener("higher:insert-attachment", h);
      window.removeEventListener("higher:detach-attachment", d);
    };
  }, [insertExisting, detachAttachment]);

  const toolbar = useMemo(() => {
    if (!editor) return null;
    const Btn = ({
      label,
      active,
      onClick,
      title,
    }: {
      label: string;
      active?: boolean;
      onClick: () => void;
      title?: string;
    }) => (
      <button
        className={"btn btn--small hdoc__tb-btn" + (active ? " hdoc__tb-btn--active" : "")}
        onMouseDown={(e) => {
          e.preventDefault();
          onClick();
        }}
        title={title}
      >
        {label}
      </button>
    );
    return (
      <div className="hdoc__toolbar">
        <Btn label="正文" active={editor.isActive("paragraph")} onClick={() => editor.chain().focus().setParagraph().run()} title="普通正文" />
        <Btn label="H2" active={editor.isActive("heading", { level: 2 })} onClick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()} />
        <Btn label="H3" active={editor.isActive("heading", { level: 3 })} onClick={() => editor.chain().focus().toggleHeading({ level: 3 }).run()} />
        <Btn label="· 列表" active={editor.isActive("bulletList")} onClick={() => editor.chain().focus().toggleBulletList().run()} title="无序列表" />
        <Btn label="1. 列表" active={editor.isActive("orderedList")} onClick={() => editor.chain().focus().toggleOrderedList().run()} title="有序列表" />
        <Btn label="</> 代码" active={editor.isActive("codeBlock")} onClick={() => editor.chain().focus().toggleCodeBlock().run()} title="代码/命令块" />
        <Btn label="图片" onClick={() => void uploadImage()} title="上传图片" />
        <Btn label="视频" onClick={() => void uploadVideo()} title="上传视频" />
        <Btn label="画图" onClick={() => setShowDraw(true)} title="画图并插入" />
        {busy && <span className="muted">{busy}</span>}
        <span className="leditor__hint">支持 Ctrl+V 粘贴截图、拖入图片/视频</span>
      </div>
    );
  }, [editor, busy, uploadImage, uploadVideo]);

  if (!editor) return <div className="muted">编辑器加载中…</div>;

  return (
    <div className={"hdoc" + (dragOver ? " hdoc--drag" : "")}>
      {!readOnly && toolbar}
      {error && (
        <div className="alert alert--error" onClick={() => setError("")}>
          {error}（点击关闭）
        </div>
      )}
      <EditorContent editor={editor} />
      {readOnly && documentToPlainText(editor.getJSON()) === "" && (
        <p className="muted">（本次学习没有笔记）</p>
      )}
      {dragOver && <div className="leditor__dropzone">松开以插入图片 / 视频</div>}
      {showDraw && sessionId != null && (
        <DrawModal
          onClose={() => setShowDraw(false)}
          onSave={async (dataUrl) => {
            const att = await saveDrawingAttachment({
              profileId,
              learningItemId: learningItemId ?? null,
              sessionId,
              dataBase64: dataUrl,
            });
            setShowDraw(false);
            insertMedia("image", att.id, att.file_name);
          }}
        />
      )}
    </div>
  );
}
