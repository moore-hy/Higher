import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import CuedRecallExperience from "../../src/components/training/CuedRecallExperience";
import ErrorCorrectionExperience from "../../src/components/training/ErrorCorrectionExperience";
import ExplainBackExperience from "../../src/components/training/ExplainBackExperience";
import FadedExampleExperience from "../../src/components/training/FadedExampleExperience";
import FreeRecallExperience from "../../src/components/training/FreeRecallExperience";
import StandardPracticeExperience from "../../src/components/training/StandardPracticeExperience";
import TrainingExperienceDispatch, {
  SPECIALIZED_PROTOCOLS,
  componentNameForProtocol,
} from "../../src/components/training/TrainingExperienceDispatch";
import TransferChallengeExperience from "../../src/components/training/TransferChallengeExperience";
import WorkedExampleExperience from "../../src/components/training/WorkedExampleExperience";
import type { ExperienceProps, ExperienceSubmit } from "../../src/components/training/experienceTypes";
import type {
  BlockCompletionState,
  GroundedProvenanceLabel,
  GroundedTrainingMaterial,
  TrainingBlockRun,
  TrainingInteraction,
} from "../../src/types";

/**
 * GROUNDED LEARNING BRIDGE V1 · W5 —— 八个专项体验消费**真实落库材料**（§10）。
 *
 * 覆盖任务书 §10 的 UI 断言：
 *
 * ```text
 * GB-UX-01 free recall hides reference before attempt
 * GB-UX-02 free recall may reveal source after attempt
 * GB-UX-03 cued recall uses persisted cue
 * GB-UX-04 worked example uses persisted material and viewing alone creates no mastery evidence
 * GB-UX-05 faded example hides the persisted step
 * GB-UX-06 standard practice uses practice semantics
 * GB-UX-07 error correction never fabricates previous error
 * GB-UX-08 explain-back feedback is not authoritative evidence
 * GB-UX-09 transfer uses a genuinely separate persisted scenario
 * GB-UX-10 generic fallback preserves original ProtocolId
 * ```
 *
 * 这里只验**界面**：材料是给定的落库快照（W3/W4 已由 Rust 侧测试保证其来源真实）。
 * 「渲染材料不产生掌握证据」在 UI 侧的可验证含义是：**渲染与提交的类型**正确 ——
 * 看例题只提交 `example_view`，练习只提交 `practice`，等等。
 */

// ============================ fixtures ============================

const EXCERPT = "光合作用把光能转成化学能，并释放氧气。";
const REFERENCE = "叶绿体是光合作用的主要场所。";

function makeBlock(over: Partial<TrainingBlockRun> = {}): TrainingBlockRun {
  return {
    id: 101,
    profile_id: 7,
    training_run_id: 55,
    ordinal: 0,
    protocol_id: "free_recall",
    is_break: false,
    goal: "复习光合作用",
    planned_minutes: 10,
    memory_unit_id: null,
    status: "active",
    started_at: null,
    ended_at: null,
    created_at: "2026-09-18 09:00:00",
    updated_at: "2026-09-18 09:00:00",
    ...over,
  };
}

function makeMaterial(over: Partial<GroundedTrainingMaterial> = {}): GroundedTrainingMaterial {
  return {
    version: 1,
    status: "ready",
    protocol_id: "free_recall",
    prompt_text: null,
    cue_text: null,
    source_excerpt: EXCERPT,
    reference_text: REFERENCE,
    worked_steps: [],
    hidden_step_index: null,
    practice_prompt: null,
    transfer_prompt: null,
    generated_by: "deterministic",
    provenance: [{ source_id: 71, revision_id: 72, section_id: 73, chunk_id: 74 }],
    unavailable_reason: null,
    ...over,
  };
}

const LABELS: GroundedProvenanceLabel[] = [
  { source_id: 71, display_name: "生物笔记.md", section_id: 73, section_title: "第三章" },
];

function makeInteraction(over: Partial<TrainingInteraction> = {}): TrainingInteraction {
  return {
    id: 1,
    profile_id: 7,
    training_run_id: 55,
    block_run_id: 101,
    client_action_id: "k1",
    interaction_type: "recall",
    prompt_text: null,
    user_response_text: null,
    hint_level: null,
    result: null,
    effect_summary_json: "{}",
    created_at: "2026-09-18 09:05:00",
    ...over,
  };
}

function props(over: Partial<ExperienceProps> = {}): ExperienceProps {
  return {
    block: makeBlock(),
    completion: null,
    interactions: [],
    response: "",
    onResponseChange: () => {},
    canSubmit: true,
    blockTerminal: false,
    busy: false,
    defaultInteractionType: "recall",
    material: null,
    provenanceLabels: [],
    submit: () => {},
    ...over,
  };
}

const provenanceText = () => screen.queryByTestId("hc-train-provenance")?.textContent ?? "";

// ============================ GB-UX-01 / 02 ============================

describe("§10.1 free_recall", () => {
  it("GB-UX-01：尝试之前不渲染参考摘录，也不提供揭晓入口", () => {
    render(<FreeRecallExperience {...props({ material: makeMaterial(), provenanceLabels: LABELS })} />);

    // 隐藏 = 不渲染（而不是 CSS 盖住）。
    expect(screen.queryByTestId("hc-train-revealed-excerpt")).toBeNull();
    expect(screen.queryByTestId("hc-train-revealed-reference")).toBeNull();
    expect(screen.queryByText(EXCERPT)).toBeNull();
    expect(screen.queryByText(REFERENCE)).toBeNull();
    // 没有尝试，就没有揭晓按钮 —— 也不给一个按了没反应的假按钮。
    expect(screen.queryByTestId("hc-train-reveal-reference")).toBeNull();
    expect(provenanceText()).toBe("");
  });

  it("GB-UX-02：尝试之后可以揭晓真实材料与出处", () => {
    render(
      <FreeRecallExperience
        {...props({
          material: makeMaterial(),
          provenanceLabels: LABELS,
          response: "光能变成化学能",
        })}
      />,
    );

    // 还没有揭晓时依然不可见。
    expect(screen.queryByTestId("hc-train-revealed-excerpt")).toBeNull();

    const reveal = screen.getByTestId("hc-train-reveal-reference");
    fireEvent.click(reveal);

    expect(screen.getByTestId("hc-train-revealed-excerpt").textContent).toContain(EXCERPT);
    expect(provenanceText()).toContain("生物笔记.md");
    expect(provenanceText()).toContain("第三章");
    // 出处行只给可读标签，不暴露内部 id。
    expect(provenanceText()).not.toContain("71");
    expect(provenanceText()).not.toContain("73");
  });
});

// ============================ GB-UX-03 ============================

describe("§10.2 cued_recall", () => {
  it("GB-UX-03：使用落库的线索，且完整参考在尝试前保持隐藏", () => {
    const cue = "线索：先想能量去了哪里";
    render(
      <CuedRecallExperience
        {...props({
          block: makeBlock({ protocol_id: "cued_recall" }),
          material: makeMaterial({ cue_text: cue }),
          provenanceLabels: LABELS,
        })}
      />,
    );

    expect(screen.getByTestId("hc-train-cue").textContent).toBe(cue);
    // 完整参考仍然隐藏。
    expect(screen.queryByTestId("hc-train-revealed-excerpt")).toBeNull();
    expect(screen.queryByText(EXCERPT)).toBeNull();
  });

  it("GB-UX-03b：没有可用线索时不编造，而是如实说明", () => {
    render(
      <CuedRecallExperience
        {...props({
          block: makeBlock({ protocol_id: "cued_recall" }),
          material: makeMaterial({ cue_text: null }),
        })}
      />,
    );

    expect(screen.queryByTestId("hc-train-cue")).toBeNull();
    expect(screen.getByText(/不会替你编一条线索/)).toBeTruthy();
    // 退回到这个块真实的编排目标，而不是编出来的线索。
    expect(screen.getByText("复习光合作用")).toBeTruthy();
  });
});

// ============================ GB-UX-04 ============================

describe("§10.3 worked_example", () => {
  it("GB-UX-04：渲染落库材料，且「只看」只提交 example_view", () => {
    const submit = vi.fn();
    render(
      <WorkedExampleExperience
        {...props({
          block: makeBlock({ protocol_id: "worked_example" }),
          material: makeMaterial({
            prompt_text: "例题：为什么叶片在光下会产生气泡？",
            worked_steps: ["观察现象", "提出假设", "设计对照"],
          }),
          provenanceLabels: LABELS,
          submit,
        })}
      />,
    );

    // 真实材料被渲染出来。
    expect(screen.getByTestId("hc-train-worked-prompt").textContent).toContain("气泡");
    expect(screen.getByTestId("hc-train-worked-steps").textContent).toContain("设计对照");
    expect(provenanceText()).toContain("生物笔记.md");
    // 只是渲染不产生任何提交。
    expect(submit).not.toHaveBeenCalled();

    fireEvent.click(screen.getByText("我看完了这一步"));

    expect(submit).toHaveBeenCalledTimes(1);
    const args = submit.mock.calls[0][0] as ExperienceSubmit;
    expect(args.interactionType).toBe("example_view");
    expect(args.result).toBeNull();
    // 「看完了」绝不是掌握类动作。
    expect(["recall", "practice", "transfer", "explanation"]).not.toContain(args.interactionType);
  });

  it("GB-UX-04b：没有可用例题材料时显示诚实的不可用状态", () => {
    render(
      <WorkedExampleExperience
        {...props({ block: makeBlock({ protocol_id: "worked_example" }), material: null })}
      />,
    );

    expect(screen.getByText(/这里暂时没有可用的例题步骤/)).toBeTruthy();
    expect(screen.queryByTestId("hc-train-worked-steps")).toBeNull();
  });
});

// ============================ GB-UX-05 ============================

describe("§10.4 faded_example", () => {
  it("GB-UX-05：只遮住快照里落库的那一步", () => {
    render(
      <FadedExampleExperience
        {...props({
          block: makeBlock({ protocol_id: "faded_example" }),
          material: makeMaterial({
            worked_steps: ["第一步：读题", "第二步：列式", "第三步：求解"],
            hidden_step_index: 1,
          }),
          provenanceLabels: LABELS,
        })}
      />,
    );

    const list = screen.getByTestId("hc-train-worked-steps");
    const items = list.querySelectorAll("li");
    expect(items.length).toBe(3);

    // 遮住的**恰好**是落库的那一步。
    expect(items[1].getAttribute("data-hidden")).toBe("true");
    expect(items[1].textContent).toContain("被遮住了");
    expect(items[1].textContent).not.toContain("第二步：列式");
    // 其余步骤可见。
    expect(items[0].textContent).toContain("第一步：读题");
    expect(items[2].textContent).toContain("第三步：求解");
    // 被遮的那一步文本不得出现在 DOM 里。
    expect(screen.queryByText("第二步：列式")).toBeNull();
    expect(provenanceText()).toContain("第三章");
  });

  it("GB-UX-05b：快照没说遮哪一步（或越界）时不编造缺步", () => {
    const { unmount } = render(
      <FadedExampleExperience
        {...props({
          block: makeBlock({ protocol_id: "faded_example" }),
          material: makeMaterial({
            worked_steps: ["甲", "乙"],
            hidden_step_index: null,
          }),
        })}
      />,
    );

    expect(screen.queryByTestId("hc-train-worked-steps")).toBeNull();
    expect(screen.getByText(/不会替你编一个缺步/)).toBeTruthy();
    unmount();

    // 越界索引同样不产生缺口。
    render(
      <FadedExampleExperience
        {...props({
          block: makeBlock({ protocol_id: "faded_example" }),
          material: makeMaterial({ worked_steps: ["甲", "乙"], hidden_step_index: 9 }),
        })}
      />,
    );
    expect(screen.queryByTestId("hc-train-hidden-step")).toBeNull();
    expect(screen.getByText(/不会替你编一个缺步/)).toBeTruthy();
  });
});

// ============================ GB-UX-06 ============================

describe("§10.5 standard_practice", () => {
  it("GB-UX-06：使用落库题面，且提交始终是 practice 语义", () => {
    const submit = vi.fn();
    render(
      <StandardPracticeExperience
        {...props({
          block: makeBlock({ protocol_id: "standard_practice" }),
          material: makeMaterial({ practice_prompt: "练习：写出该反应的方程式。" }),
          provenanceLabels: LABELS,
          submit,
        })}
      />,
    );

    expect(screen.getByTestId("hc-train-practice-prompt").textContent).toContain("方程式");
    expect(screen.getByText(/作为\*\*练习\*\*被如实记录（`PracticeSuccess`）/)).toBeTruthy();

    fireEvent.click(screen.getByText("提交这次练习"));
    const args = submit.mock.calls[0][0] as ExperienceSubmit;
    expect(args.interactionType).toBe("practice");
    // 练习成功永远不会被讲成一次回忆。
    expect(args.interactionType).not.toBe("recall");
  });

  it("GB-UX-06b：没有落库题面时不凭空生成一道题", () => {
    render(
      <StandardPracticeExperience
        {...props({ block: makeBlock({ protocol_id: "standard_practice" }), material: null })}
      />,
    );

    expect(screen.getByText(/这里暂时没有可用的题面/)).toBeTruthy();
    expect(screen.queryByTestId("hc-train-practice-prompt")).toBeNull();
  });
});

// ============================ GB-UX-07 ============================

describe("§10.6 error_correction", () => {
  it("GB-UX-07：没有真实错误时不编造，且不开放作答区", () => {
    render(
      <ErrorCorrectionExperience
        {...props({
          block: makeBlock({ protocol_id: "error_correction" }),
          material: makeMaterial(),
        })}
      />,
    );

    expect(screen.queryByTestId("hc-train-prior-error")).toBeNull();
    expect(screen.getByText(/这里暂时没有可用的上一次的错误/)).toBeTruthy();
    expect(screen.getByText(/不会替你编一个错误来纠正/)).toBeTruthy();
    expect(screen.queryByLabelText("错在哪里 / 怎么改")).toBeNull();
  });

  it("GB-UX-07b：真实错误来自落库交互，材料只作参考上下文", () => {
    render(
      <ErrorCorrectionExperience
        {...props({
          block: makeBlock({ protocol_id: "error_correction" }),
          material: makeMaterial(),
          provenanceLabels: LABELS,
          interactions: [
            makeInteraction({ id: 9, interaction_type: "recall", result: "failure", user_response_text: "我把光能写成了热能" }),
          ],
        })}
      />,
    );

    // 纠错对象是**真实落库**的那次错误。
    expect(screen.getByTestId("hc-train-prior-error").textContent).toContain("我把光能写成了热能");
    // 材料被显式标注为文档原文，而不是用户的错误。
    const ref = screen.getByTestId("hc-train-error-reference");
    expect(ref.textContent).toContain(EXCERPT);
    expect(ref.textContent).toContain("不是你的错误");
    expect(provenanceText()).toContain("生物笔记.md");
  });
});

// ============================ GB-UX-08 ============================

describe("§10.7 explain_back", () => {
  it("GB-UX-08：讲完之后对照材料，但材料不是「标准答案」，反馈也不权威", () => {
    const submit = vi.fn();
    render(
      <ExplainBackExperience
        {...props({
          block: makeBlock({ protocol_id: "explain_back" }),
          material: makeMaterial(),
          provenanceLabels: LABELS,
          response: "光合作用是把光能存进有机物里",
          submit,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("hc-train-reveal-reference"));

    const excerpt = screen.getByTestId("hc-train-revealed-excerpt");
    expect(excerpt.textContent).toContain(EXCERPT);
    // 对照物被明确说成「不是标准答案」。
    expect(excerpt.textContent).toContain("不是标准答案");
    expect(screen.getByText(/没有\*\*对你的解释做过语义判定/)).toBeTruthy();
    // 绝不出现假的 AI 权威措辞。
    expect(screen.queryByText(/AI 已评分/)).toBeNull();
    expect(screen.queryByText(/系统已确认/)).toBeNull();

    fireEvent.click(screen.getByText("提交我的解释"));
    const args = submit.mock.calls[0][0] as ExperienceSubmit;
    expect(args.interactionType).toBe("explanation");
  });
});

// ============================ GB-UX-09 ============================

describe("§10.8 transfer_challenge", () => {
  it("GB-UX-09：使用落库的迁移情境，且生成情境与概念来源分开呈现", () => {
    const scenario = "把光合作用的原理搬到深海热泉生态里解释。";
    render(
      <TransferChallengeExperience
        {...props({
          block: makeBlock({ protocol_id: "transfer_challenge" }),
          material: makeMaterial({
            transfer_prompt: scenario,
            generated_by: "ai_non_authoritative",
          }),
          provenanceLabels: LABELS,
        })}
      />,
    );

    const scenarioBlock = screen.getByTestId("hc-train-transfer-scenario");
    const conceptBlock = screen.getByTestId("hc-train-transfer-concept-source");

    // 情境与来源是**两段不同的东西** —— 生成的情境不冒充来源原文。
    expect(scenarioBlock.textContent).toContain(scenario);
    expect(conceptBlock.textContent).toContain(EXCERPT);
    expect(scenarioBlock.textContent).not.toContain(EXCERPT);

    // 生成这件事必须说出来，且仍然保留指向概念来源的出处。
    expect(screen.getByTestId("hc-train-material-origin").textContent).toContain("AI");
    expect(provenanceText()).toContain("生物笔记.md");
  });

  it("GB-UX-09b：没有落库迁移情境时不用原题换个说法充数", () => {
    render(
      <TransferChallengeExperience
        {...props({
          block: makeBlock({ protocol_id: "transfer_challenge" }),
          material: makeMaterial({ transfer_prompt: null }),
        })}
      />,
    );

    expect(screen.getByText(/这里暂时没有可用的迁移题面/)).toBeTruthy();
    expect(screen.getByText(/不会把原来的题换个说法充数/)).toBeTruthy();
    expect(screen.queryByTestId("hc-train-transfer-scenario")).toBeNull();
  });
});

// ============================ GB-UX-10 ============================

describe("§10.9 通用兜底", () => {
  it("GB-UX-10：兜底保留原 ProtocolId 与冻结完成规则，且八个专项各不相同", () => {
    const completion: BlockCompletionState = {
      block_run_id: 101,
      rule_kind: "completion_rule_kind_placeholder" as unknown as BlockCompletionState["rule_kind"],
      rule_zh: "按冻结规则判定",
      satisfied: false,
      reason: "not_yet_satisfied",
    };

    render(
      <TrainingExperienceDispatch
        {...props({
          block: makeBlock({ protocol_id: "translation_guided" }),
          completion,
          defaultInteractionType: "practice",
        })}
      />,
    );

    // 原 ProtocolId 没有被静默换成别的协议。
    const text = document.body.textContent ?? "";
    expect(text).toContain("translation_guided");

    // 八个专项协议各自映射到**互不相同**的组件（不是八个 id 落一个地方）。
    const names = SPECIALIZED_PROTOCOLS.map((p) => componentNameForProtocol(p));
    expect(new Set(names).size).toBe(SPECIALIZED_PROTOCOLS.length);
    expect(names).not.toContain("GenericGuidedExperience");
    expect(componentNameForProtocol("translation_guided")).toBe("GenericGuidedExperience");
    expect(componentNameForProtocol(null)).toBe("GenericGuidedExperience");
  });
});
