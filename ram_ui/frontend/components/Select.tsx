import {
  useEffect,
  useId,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { Icon } from "./Icon";

type SelectOption = {
  value: string;
  label: string;
  disabled?: boolean;
};

type SelectProps = {
  value: string;
  options: SelectOption[];
  onChange: (value: string) => void;
  ariaLabel: string;
  disabled?: boolean;
  animated?: boolean;
};

export default function Select({
  value,
  options,
  onChange,
  ariaLabel,
  disabled = false,
  animated = true,
}: SelectProps) {
  const listboxId = useId();
  const rootRef = useRef<HTMLDivElement>(null);
  const animationFrameRef = useRef<number | null>(null);
  const [isMenuMounted, setIsMenuMounted] = useState(false);
  const [isOpen, setIsOpen] = useState(false);
  const [highlightedIndex, setHighlightedIndex] = useState(() =>
    Math.max(0, options.findIndex((option) => option.value === value)),
  );
  const shouldAnimate =
    animated && !window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  const selectedOption =
    options.find((option) => option.value === value) ?? options[0];

  function closeMenu() {
    if (animationFrameRef.current !== null) {
      window.cancelAnimationFrame(animationFrameRef.current);
      animationFrameRef.current = null;
    }
    setIsOpen(false);
    if (!shouldAnimate) setIsMenuMounted(false);
  }

  function openMenu() {
    if (disabled) return;
    setHighlightedIndex(
      Math.max(0, options.findIndex((option) => option.value === value)),
    );
    setIsMenuMounted(true);
    if (!shouldAnimate) {
      setIsOpen(true);
      return;
    }
    animationFrameRef.current = window.requestAnimationFrame(() => {
      animationFrameRef.current = null;
      setIsOpen(true);
    });
  }

  function selectOption(option: SelectOption) {
    if (option.disabled) return;
    onChange(option.value);
    closeMenu();
  }

  function moveHighlight(direction: 1 | -1) {
    if (options.length === 0) return;
    let nextIndex = highlightedIndex;
    do {
      nextIndex = (nextIndex + direction + options.length) % options.length;
    } while (options[nextIndex]?.disabled && nextIndex !== highlightedIndex);
    setHighlightedIndex(nextIndex);
  }

  function onTriggerKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    if (event.key === "Escape" && isOpen) {
      event.preventDefault();
      closeMenu();
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!isOpen) {
        openMenu();
      } else {
        moveHighlight(event.key === "ArrowDown" ? 1 : -1);
      }
      return;
    }
    if ((event.key === "Enter" || event.key === " ") && isOpen) {
      event.preventDefault();
      const option = options[highlightedIndex];
      if (option) selectOption(option);
    }
  }

  useEffect(() => {
    if (!isMenuMounted) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) closeMenu();
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [isMenuMounted, shouldAnimate]);

  useEffect(
    () => () => {
      if (animationFrameRef.current !== null) {
        window.cancelAnimationFrame(animationFrameRef.current);
      }
    },
    [],
  );

  return (
    <div
      ref={rootRef}
      className="rm-select"
      data-animated={shouldAnimate ? "true" : "false"}
    >
      <button
        className="rm-select-trigger"
        type="button"
        aria-label={ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={isOpen}
        aria-controls={listboxId}
        aria-activedescendant={
          isOpen ? `${listboxId}-option-${highlightedIndex}` : undefined
        }
        disabled={disabled}
        onClick={() => (isOpen ? closeMenu() : openMenu())}
        onBlur={closeMenu}
        onKeyDown={onTriggerKeyDown}
      >
        <span>{selectedOption?.label ?? "Select an option"}</span>
        <Icon name="chevron-down" />
      </button>
      {isMenuMounted && (
        <div
          id={listboxId}
          className={`rm-select-menu ${isOpen ? "is-open" : "is-closed"}`}
          role="listbox"
          aria-label={ariaLabel}
          onTransitionEnd={(event) => {
            if (event.target === event.currentTarget && !isOpen) {
              setIsMenuMounted(false);
            }
          }}
        >
          {options.map((option, index) => (
            <button
              id={`${listboxId}-option-${index}`}
              className={`rm-select-option ${
                option.value === value ? "is-selected" : ""
              } ${index === highlightedIndex ? "is-highlighted" : ""}`}
              key={option.value}
              type="button"
              role="option"
              aria-selected={option.value === value}
              disabled={option.disabled}
              tabIndex={-1}
              onPointerDown={(event) => event.preventDefault()}
              onPointerEnter={() => setHighlightedIndex(index)}
              onClick={() => selectOption(option)}
            >
              {option.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
