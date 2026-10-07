import { useEffect, useRef, useState } from "react";
import "./Walkthrough.css";

export const walkthroughSteps = [
  { page: "Accounts", target: "navigation", title: "Welcome to Roblox Manager", description: "Use this rail to move between your workspaces. We'll show you the main controls. You can finish this tour without adding an account or launching Roblox." },
  { page: "Accounts", target: "add-account", title: "Add your accounts", description: "Use the plus button to log in or paste a cookie. Adding an existing account updates its login and keeps its saved settings." },
  { page: "Accounts", target: "accounts", title: "Choose your accounts", description: "Choose Builderman or Roblox to explore the launch controls. These are example accounts. Your own accounts will appear here after the tour." },
  { page: "Accounts", target: "launch", title: "Launch a game", description: "After selecting an account, enter a numeric Place ID and choose Launch. Presets and Private Servers save destinations for later. This tour won't launch anything." },
  { page: "Instances", target: "instances", title: "Track your clients", description: "Track running clients and launch progress here. Clients marked Verified can be closed individually; Estimated means the account is a guess. You can focus windows, arrange them or join a server." },
  { page: "Settings", target: "preferences", title: "Make RM work for you", description: "Settings covers account storage, privacy and window arrangement. Use its sidebar to explore each section. Keep your encrypted account store and backups safe. You're ready to add an account." },
] as const;

type Highlight = { left: number; top: number; width: number; height: number; hasTarget: boolean };
type WalkthroughProps = {
  stepIndex: number;
  isPageReady: boolean;
  isPending: boolean;
  error: string | null;
  onBack: () => void;
  onNext: () => void;
  onFinish: () => void;
  onReturnToStep: () => void;
};

export function Walkthrough({ stepIndex, isPageReady, isPending, error, onBack, onNext, onFinish, onReturnToStep }: WalkthroughProps) {
  const dock = useRef<HTMLElement>(null);
  const [highlight, setHighlight] = useState<Highlight | null>(null);
  const step = walkthroughSteps[stepIndex];
  const isLastStep = stepIndex === walkthroughSteps.length - 1;
  const [example, setExample] = useState<{ stepIndex: number; message: string } | null>(null);

  useEffect(() => {
    const content = document.querySelector<HTMLElement>("[data-walkthrough-content]");
    const previousFocus = document.activeElement;
    dock.current?.focus();
    return () => {
      if (content) {
        content.style.removeProperty("--walkthrough-space");
      }
      if (previousFocus instanceof HTMLElement && previousFocus.isConnected) previousFocus.focus();
      else content?.querySelector<HTMLElement>("[aria-current='page']")?.focus();
    };
  }, []);

  useEffect(() => {
    const content = document.querySelector<HTMLElement>("[data-walkthrough-content]");
    if (!content) return;
    function onShowcaseClick(event: Event) {
      if (!isPending && event.target instanceof Element) {
        const button = event.target.closest<HTMLElement>("button");
        if (button?.dataset.walkthroughPage || button?.dataset.showcaseAction === "select-demo-account") return;
      }
      event.preventDefault();
      event.stopImmediatePropagation();
      if (isPending || !(event.target instanceof Element)) return;
      const target = event.target.closest<HTMLElement>("[data-walkthrough]");
      if (target?.dataset.walkthrough === "navigation") {
        setExample({ stepIndex, message: "The rail opens workspaces without advancing the tour. Use Next to cover each step in order." });
      } else if (target?.dataset.walkthrough === "add-account") {
        setExample({ stepIndex, message: "Add account offers browser sign-in or a pasted cookie. This is a preview, so no sign-in or input is needed. Choose Next whenever you're ready." });
      } else if (target?.dataset.walkthrough === "launch") {
        setExample({ stepIndex, message: "Launch starts the selected account in the chosen game. In this showcase, nothing is launched and no Place ID is needed." });
      } else {
        setExample({ stepIndex, message: "This is a showcase. You can explore the buttons without changing accounts, settings or running clients. Choose Next to continue." });
      }
    }
    function onShowcaseInput(event: Event) {
      event.preventDefault();
      event.stopImmediatePropagation();
    }
    function onShowcaseKeyDown(event: KeyboardEvent) {
      if (event.target instanceof Element && event.target.matches("input, textarea, select, [contenteditable]")) {
        if (event.key !== "Tab" && event.key !== "Escape") onShowcaseInput(event);
      }
      if (event.key === "Escape" && !isPending) { onShowcaseInput(event); onFinish(); }
    }
    content.addEventListener("click", onShowcaseClick, true);
    content.addEventListener("beforeinput", onShowcaseInput, true);
    content.addEventListener("keydown", onShowcaseKeyDown, true);
    content.addEventListener("dragstart", onShowcaseInput, true);
    content.addEventListener("contextmenu", onShowcaseInput, true);
    content.addEventListener("submit", onShowcaseInput, true);
    return () => {
      content.removeEventListener("click", onShowcaseClick, true);
      content.removeEventListener("beforeinput", onShowcaseInput, true);
      content.removeEventListener("keydown", onShowcaseKeyDown, true);
      content.removeEventListener("dragstart", onShowcaseInput, true);
      content.removeEventListener("contextmenu", onShowcaseInput, true);
      content.removeEventListener("submit", onShowcaseInput, true);
    };
  }, [isPending, onFinish, stepIndex]);

  useEffect(() => {
    const content = document.querySelector<HTMLElement>("[data-walkthrough-content]");
    const panel = dock.current;
    if (!content || !panel) return;
    function reserveDockSpace() {
      if (content && panel) content.style.setProperty("--walkthrough-space", `${panel.getBoundingClientRect().height + 24}px`);
    }
    const observer = new ResizeObserver(reserveDockSpace);
    observer.observe(panel);
    reserveDockSpace();
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (!isPageReady) return;
    const content = document.querySelector<HTMLElement>("[data-walkthrough-content]");
    if (!content) return;
    let frame = 0;
    let previousTarget: HTMLElement | null = null;
    const observer = new ResizeObserver(scheduleMeasurement);
    observer.observe(content);
    if (dock.current) observer.observe(dock.current);

    function measureTarget() {
      const target = content?.querySelector<HTMLElement>(`[data-walkthrough="${step.target}"]`);
      const fallback = content?.querySelector<HTMLElement>(step.target === "launch" ? "[data-walkthrough='account-details']" : "[data-walkthrough='workspace'] .page-transition-viewport");
      const element = target ?? fallback;
      if (!element) { setHighlight(null); return; }
      const pageLayer = element.closest(".page-transition-layer");
      if (pageLayer?.getAnimations().some((animation) => animation.playState === "running")) {
        setHighlight(null);
        scheduleMeasurement();
        return;
      }
      if (element !== previousTarget) {
        if (previousTarget) observer.unobserve(previousTarget);
        previousTarget = element;
        observer.observe(element);
        if (target && step.target !== "navigation") target.scrollIntoView({ block: "nearest", inline: "nearest", behavior: "instant" });
      }
      const rectangle = element.getBoundingClientRect();
      let left = Math.max(2, rectangle.left - 4);
      let top = Math.max(40, rectangle.top - 4);
      let right = Math.min(window.innerWidth - 2, rectangle.right + 4);
      let bottom = Math.min(window.innerHeight - 2, rectangle.bottom + 4);
      for (let parent = element.parentElement; parent && parent !== content; parent = parent.parentElement) {
        const style = getComputedStyle(parent);
        const bounds = parent.getBoundingClientRect();
        if (/(auto|scroll|hidden|clip)/.test(style.overflowX)) { left = Math.max(left, bounds.left); right = Math.min(right, bounds.right); }
        if (/(auto|scroll|hidden|clip)/.test(style.overflowY)) { top = Math.max(top, bounds.top); bottom = Math.min(bottom, bounds.bottom); }
      }
      const next = { left, top, width: Math.max(0, right - left), height: Math.max(0, bottom - top), hasTarget: !!target };
      setHighlight((current) => current && Object.keys(next).every((key) => current[key as keyof Highlight] === next[key as keyof Highlight]) ? current : next);
    }
    function scheduleMeasurement() {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(measureTarget);
    }
    const mutations = new MutationObserver(scheduleMeasurement);
    mutations.observe(content, { childList: true, subtree: true });
    window.addEventListener("resize", scheduleMeasurement);
    window.addEventListener("scroll", scheduleMeasurement, true);
    scheduleMeasurement();
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      mutations.disconnect();
      window.removeEventListener("resize", scheduleMeasurement);
      window.removeEventListener("scroll", scheduleMeasurement, true);
    };
  }, [isPageReady, step.target]);

  const visibleHighlight = isPageReady ? highlight : null;
  const isPreviewVisible = step.target === "launch" && isPageReady && !visibleHighlight?.hasTarget;

  return (
    <div className="walkthrough-overlay">
      <svg className="walkthrough-shade" aria-hidden="true" width="100%" height="100%">
        <defs>
          <mask id="walkthrough-cutout">
            <rect width="100%" height="100%" fill="white" />
            {visibleHighlight && <rect className="walkthrough-hole" x={visibleHighlight.left} y={visibleHighlight.top} width={visibleHighlight.width} height={visibleHighlight.height} fill="black" />}
          </mask>
        </defs>
        <rect width="100%" height="100%" className="walkthrough-dim" mask="url(#walkthrough-cutout)" />
        {visibleHighlight && <rect className="walkthrough-outline" x={visibleHighlight.left} y={visibleHighlight.top} width={visibleHighlight.width} height={visibleHighlight.height} />}
      </svg>
      <section
        ref={dock}
        className="walkthrough-dock"
        role="region"
        aria-labelledby="walkthrough-title"
        aria-describedby="walkthrough-description"
        aria-busy={isPending}
        tabIndex={-1}
        onKeyDown={(event) => {
          if (event.key === "Escape" && !isPending) { event.preventDefault(); onFinish(); }
        }}
      >
        <div className="walkthrough-copy" aria-live="polite" aria-atomic="true">
          <span className="walkthrough-progress">Getting started / {stepIndex + 1} of {walkthroughSteps.length}</span>
          <h2 id="walkthrough-title">{step.title}</h2>
          <p id="walkthrough-description">{step.description}</p>
          <p className="walkthrough-showcase-hint">Click controls to preview them. No input is needed. The rail lets you explore; Next continues each step in order.</p>
        </div>
        {!isPageReady && <div className="walkthrough-preview">
          <span>This step is on {step.page}. Exploring another page doesn't advance the tour.</span>
          <button className="account-button" type="button" disabled={isPending} onClick={onReturnToStep}>Return to {step.page}</button>
        </div>}
        <div className="walkthrough-actions">
          <button className="walkthrough-skip" type="button" disabled={isPending} onClick={onFinish}>Skip tour</button>
          <button className="account-button" type="button" disabled={stepIndex === 0 || isPending} onClick={onBack}>Back</button>
          <button className="account-button walkthrough-next" type="button" disabled={isPending} onClick={isLastStep ? onFinish : onNext}>{isPending ? "Saving..." : isLastStep ? "Finish" : "Next"}</button>
        </div>
        {isPreviewVisible && <div className="walkthrough-preview">
          <span>Preview / select an account to see these controls</span>
          <label>Place ID <input disabled placeholder="Enter a Place ID" /></label>
          <button className="account-button" type="button" onClick={() => setExample({ stepIndex, message: "Launch starts the selected account in the chosen game. This preview doesn't launch Roblox or need a Place ID." })}>Launch</button>
        </div>}
        {example?.stepIndex === stepIndex && <p className="walkthrough-example" role="status">{example.message}</p>}
        {example?.stepIndex === stepIndex && step.target === "add-account" && <div className="walkthrough-preview">
          <span>Add account preview</span>
          <button className="account-button" type="button" onClick={() => setExample({ stepIndex, message: "Log in through a Roblox browser window to add an account. This tour won’t open a login window." })}>Log in through browser</button>
          <button className="account-button" type="button" onClick={() => setExample({ stepIndex, message: "Use Paste cookie to add an account with a Roblox cookie. This tour won’t ask for a real cookie." })}>Paste cookie</button>
        </div>}
        {error && <p className="walkthrough-error" role="alert">{error} Choose Skip tour or Finish to retry.</p>}
      </section>
    </div>
  );
}
