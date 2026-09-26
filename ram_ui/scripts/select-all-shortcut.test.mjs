import assert from "node:assert/strict";
import test from "node:test";
import { isSelectAllShortcut, isTextSelectionTarget } from "../frontend/lib/selectAllShortcut.ts";

const key = { key: "a", ctrlKey: true, metaKey: false, altKey: false, shiftKey: false, isComposing: false };

test("only the select-all shortcut is recognised", () => {
  assert.equal(isSelectAllShortcut(key), true);
  assert.equal(isSelectAllShortcut({ ...key, key: "A" }), true);
  assert.equal(isSelectAllShortcut({ ...key, ctrlKey: false, metaKey: true }), true);
  for (const change of [{ ctrlKey: false }, { key: "s" }, { altKey: true }, { shiftKey: true }, { isComposing: true }]) {
    assert.equal(isSelectAllShortcut({ ...key, ...change }), false);
  }
});

class TestElement {
  closest(selector) { return selector.includes(this.tagName) ? this : null; }
}
class TestHTMLElement extends TestElement {}
class TestInput extends TestHTMLElement { tagName = "input"; type = "text"; }

test("text fields keep native select-all while controls use page selection", () => {
  const originals = { Element: globalThis.Element, HTMLElement: globalThis.HTMLElement, HTMLInputElement: globalThis.HTMLInputElement };
  Object.assign(globalThis, { Element: TestElement, HTMLElement: TestHTMLElement, HTMLInputElement: TestInput });
  try {
    assert.equal(isTextSelectionTarget(null), false);
    assert.equal(isTextSelectionTarget(new TestInput()), true);
    for (const type of ["search", "password", "number", "url", "email"]) {
      assert.equal(isTextSelectionTarget(Object.assign(new TestInput(), { type })), true);
    }
    for (const type of ["checkbox", "radio", "button", "file", "range"]) {
      assert.equal(isTextSelectionTarget(Object.assign(new TestInput(), { type })), false);
    }
    assert.equal(isTextSelectionTarget(Object.assign(new TestHTMLElement(), { tagName: "textarea" })), true);
    assert.equal(isTextSelectionTarget(Object.assign(new TestHTMLElement(), { isContentEditable: true })), true);
    assert.equal(isTextSelectionTarget(Object.assign(new TestHTMLElement(), { tagName: "button" })), false);
  } finally {
    for (const [name, original] of Object.entries(originals)) {
      if (original === undefined) delete globalThis[name];
      else globalThis[name] = original;
    }
  }
});
