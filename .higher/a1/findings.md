# A1 — findings.md（FP-2.3 未来工作记录）

> 依据 A1 FINAL PATCH FP-2.3：本任务只允许声明 `CJK FALLBACK V1 BUG FIXED` 的特定含义；
> 中文检索的进一步增强必须记录到 findings.md，且本任务不施工。

## 已修复（确定性缺陷）
- 多段 Grounding query（item 名 + 描述 + goal + domain 拼接）曾被作为一整条 `%whole_query%`
  LIKE 字符串，导致中文真实材料难以命中。
- A1 改为按 term 拆分后 OR（`%term%` 各词），并统一 LIKE 安全处理（方案 B）+ 有界 + fail-closed。

## 不属于本任务范围（明确 NOT claimed）
- Chinese retrieval fully solved —— 未达成，也不声称。
- 中文语义检索已经完善 —— 未达成。
- 中文分词已经解决 —— 未达成（`split_whitespace` + bounded LIKE 不是成熟 tokenizer）。
- 中文检索达到最终形态 —— 未达成。

## 未来增强（deferred，Third-Party First）
若后续需要中文检索达到生产级，应优先评估成熟方案，而非继续在本任务式堆启发式字符串规则：

1. **成熟中文 tokenizer / 分词**：如 jieba（Rust `jieba-rs`）、中文 n-gram、或基于词典 + HMM 的分词，
   替换当前 `split_whitespace` 对中文无效的处理。
2. **中文 lexical retrieval strategy**：在 FTS5 上评估 `unicode61` vs `trigram` tokenizer，
   或对中文 chunk 建立 trigram 索引 + BM25 调权。
3. **与既有 `document_intelligence` 中文 2-gram 策略对齐**：`ai/context_builder.rs` 已有
   “中文 2-gram + 英文单词”的检索预处理思路，可评估下沉到搜索层统一复用。
4. **评估顺序**：Third-Party First —— 先调研成熟库/策略，确认收益与维护成本后再决定是否引入；
   不在 A1 范围内施工。

## 状态
- 本任务：只修确定性 CJK FALLBACK bug；不引入新 tokenizer；不改动搜索层架构。
- 上述 deferred 项为本任务明确授权范围之外，记录于此供后续任务承接。
