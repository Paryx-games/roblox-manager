import type { AccountExportOptions } from "./ipc";

export function exportValidation(options: AccountExportOptions, confirmPassword: string): string | null {
  if (!options.accountIds.length || options.accountIds.length > 500) return "Select between 1 and 500 accounts.";
  if (!options.fields.length) return "Select at least one field.";
  if (options.fields.includes("cookie") && options.encryption === "none") return "Cookies require a recommended encryption method.";
  if (options.encryption !== "none") {
    if ([...options.password].length < 12 || new TextEncoder().encode(options.password).length > 1024 || !options.password.trim()) return "Use an export password of at least 12 characters and at most 1024 bytes.";
    if (options.password !== confirmPassword) return "The export passwords do not match.";
  } else if (options.password) {
    return "Choose an encryption method to use an export password.";
  }
  return null;
}
