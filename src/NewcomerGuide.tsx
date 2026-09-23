import { useEffect, useState } from "react";
import {
  ArrowLeft,
  ArrowRight,
  Check,
  KeyRound,
  Layers3,
  Play,
  Pause,
  ShieldCheck,
} from "lucide-react";
import { Modal } from "./components";
import "./newcomer-guide.css";

const guideSteps = [
  {
    title: "申请 WorkBuddy 密钥",
    detail:
      "从模型库点击“从 New API 添加”，登录钉钉 / Keycloak。选择 WorkBuddy、winwin 和 auto，再点击“创建并添加”。",
    cue: "登录 → 选择 → 创建",
  },
  {
    title: "在模型库找到它",
    detail:
      "创建成功后返回模型库，找到“winwin · auto”。可先点击测试模型，确认密钥可用。",
    cue: "winwin · auto 已加入模型库",
  },
  {
    title: "应用到 Agent",
    detail:
      "点击模型卡片上的“应用到 Agent”，选择 WorkBuddy，预览配置后确认覆盖并备份。",
    cue: "选择 WorkBuddy → 预览 → 确认",
  },
] as const;

/** Show an animated, keyboard-operable walkthrough that leads into the real import flow. */
export function NewcomerGuide({
  onClose,
  onStart,
}: {
  onClose: () => void;
  onStart: () => void;
}) {
  const [step, setStep] = useState(0);
  const [playing, setPlaying] = useState(true);

  useEffect(() => {
    if (!playing) return;
    const timer = window.setInterval(
      () => setStep((current) => (current + 1) % guideSteps.length),
      6000,
    );
    return () => window.clearInterval(timer);
  }, [playing]);

  /** Let manual navigation pause the animation so the selected instruction stays visible. */
  function goTo(next: number) {
    setPlaying(false);
    setStep(next);
  }

  return (
    <Modal
      title="新手指引"
      description="三步完成 WorkBuddy 密钥申请、入库与应用。"
      onClose={onClose}
      className="newcomer-modal"
      wide
    >
      <div className="newcomer-guide">
        <div className="newcomer-progress" aria-label="引导步骤">
          {guideSteps.map((item, index) => (
            <button
              key={item.title}
              className={index === step ? "active" : ""}
              aria-current={index === step ? "step" : undefined}
              onClick={() => goTo(index)}
            >
              <span>{String(index + 1).padStart(2, "0")}</span>
              {item.title}
            </button>
          ))}
        </div>
        <div key={step} className="newcomer-scene">
          <div className="newcomer-visual" aria-hidden="true">
            {step === 0 && (
              <div className="newcomer-demo-card">
                <div className="newcomer-demo-head">
                  <KeyRound size={20} /> 从 New API 添加
                </div>
                <div className="newcomer-demo-row">
                  <span>目前客户端</span>
                  <strong>WorkBuddy</strong>
                </div>
                <div className="newcomer-demo-row">
                  <span>平台名称</span>
                  <strong>winwin</strong>
                </div>
                <div className="newcomer-demo-row">
                  <span>模型</span>
                  <strong>auto</strong>
                </div>
                <div className="newcomer-demo-cta">
                  <ShieldCheck size={16} /> 创建并添加
                </div>
              </div>
            )}
            {step === 1 && (
              <div className="newcomer-demo-card newcomer-model-card">
                <span className="newcomer-demo-kicker">
                  <Layers3 size={16} /> 模型库
                </span>
                <strong>winwin · auto</strong>
                <span>WorkBuddy · OpenAI Chat</span>
                <div className="newcomer-demo-key">
                  API KEY <span>••••••••••••</span>
                </div>
                <div className="newcomer-demo-cta">
                  <Check size={16} /> 已保存到模型库
                </div>
              </div>
            )}
            {step === 2 && (
              <div className="newcomer-demo-card newcomer-apply-card">
                <span className="newcomer-demo-kicker">应用到 Agent</span>
                <strong>winwin · auto</strong>
                <div className="newcomer-agent-choice">
                  <span>
                    <Check size={16} />
                  </span>{" "}
                  WorkBuddy
                </div>
                <div className="newcomer-demo-cta">
                  预览并确认 <ArrowRight size={16} />
                </div>
              </div>
            )}
          </div>
          <div className="newcomer-copy">
            <span className="newcomer-index">
              STEP {String(step + 1).padStart(2, "0")} / 03
            </span>
            <h3>{guideSteps[step].title}</h3>
            <p>{guideSteps[step].detail}</p>
            <span className="newcomer-cue">{guideSteps[step].cue}</span>
          </div>
        </div>
        <div className="newcomer-controls">
          <button
            className="button secondary"
            aria-label={playing ? "暂停引导动画" : "播放引导动画"}
            onClick={() => setPlaying(!playing)}
          >
            {playing ? <Pause size={16} /> : <Play size={16} />}
          </button>
          <div className="newcomer-dots" aria-label="当前步骤">
            {guideSteps.map((item, index) => (
              <button
                key={item.title}
                aria-label={`查看第 ${index + 1} 步`}
                aria-current={index === step ? "step" : undefined}
                className={index === step ? "active" : ""}
                onClick={() => goTo(index)}
              />
            ))}
          </div>
          <button
            className="button secondary"
            disabled={step === 0}
            onClick={() => goTo(step - 1)}
          >
            <ArrowLeft size={16} />
            上一步
          </button>
          {step < guideSteps.length - 1 ? (
            <button className="button primary" onClick={() => goTo(step + 1)}>
              下一步
              <ArrowRight size={16} />
            </button>
          ) : (
            <button className="button guide-start" onClick={onStart}>
              开始从 New API 添加
              <ArrowRight size={16} />
            </button>
          )}
        </div>
      </div>
    </Modal>
  );
}
