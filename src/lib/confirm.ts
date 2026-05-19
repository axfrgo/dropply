type ConfirmOptions = {
  confirmLabel?: string;
  cancelLabel?: string;
  destructive?: boolean;
  title?: string;
};

export function confirmAction(message: string, options: ConfirmOptions = {}): Promise<boolean> {
  if (typeof document === "undefined") {
    return Promise.resolve(false);
  }

  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.style.position = "fixed";
    overlay.style.inset = "0";
    overlay.style.zIndex = "9999";
    overlay.style.display = "flex";
    overlay.style.alignItems = "center";
    overlay.style.justifyContent = "center";
    overlay.style.padding = "1.5rem";
    overlay.style.background = "rgba(12, 18, 28, 0.56)";

    const dialog = document.createElement("div");
    dialog.style.width = "min(28rem, 100%)";
    dialog.style.borderRadius = "1rem";
    dialog.style.border = "1px solid rgba(95, 115, 140, 0.35)";
    dialog.style.background = "#f8fbff";
    dialog.style.boxShadow = "0 32px 80px rgba(18, 32, 52, 0.22)";
    dialog.style.padding = "1.2rem";
    dialog.style.color = "#1b2433";

    if (options.title) {
      const title = document.createElement("strong");
      title.textContent = options.title;
      title.style.display = "block";
      title.style.fontSize = "1rem";
      title.style.marginBottom = "0.6rem";
      dialog.appendChild(title);
    }

    const body = document.createElement("p");
    body.textContent = message;
    body.style.margin = "0";
    body.style.lineHeight = "1.5";
    body.style.fontSize = "0.96rem";
    dialog.appendChild(body);

    const actions = document.createElement("div");
    actions.style.display = "flex";
    actions.style.justifyContent = "flex-end";
    actions.style.gap = "0.65rem";
    actions.style.marginTop = "1rem";

    const cancelButton = document.createElement("button");
    cancelButton.type = "button";
    cancelButton.textContent = options.cancelLabel ?? "Cancel";
    cancelButton.style.border = "1px solid rgba(95, 115, 140, 0.35)";
    cancelButton.style.borderRadius = "999px";
    cancelButton.style.padding = "0.6rem 0.95rem";
    cancelButton.style.background = "#ffffff";
    cancelButton.style.color = "#1b2433";
    cancelButton.style.cursor = "pointer";

    const confirmButton = document.createElement("button");
    confirmButton.type = "button";
    confirmButton.textContent = options.confirmLabel ?? "Confirm";
    confirmButton.style.border = "0";
    confirmButton.style.borderRadius = "999px";
    confirmButton.style.padding = "0.6rem 0.95rem";
    confirmButton.style.background = options.destructive ? "#c43d3d" : "#1b2433";
    confirmButton.style.color = "#ffffff";
    confirmButton.style.cursor = "pointer";

    const cleanup = (result: boolean) => {
      window.removeEventListener("keydown", handleKeyDown);
      overlay.remove();
      resolve(result);
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        cleanup(false);
      }

      if (event.key === "Enter") {
        cleanup(true);
      }
    };

    overlay.addEventListener("click", (event) => {
      if (event.target === overlay) {
        cleanup(false);
      }
    });

    cancelButton.addEventListener("click", () => cleanup(false));
    confirmButton.addEventListener("click", () => cleanup(true));
    window.addEventListener("keydown", handleKeyDown);

    actions.append(cancelButton, confirmButton);
    dialog.appendChild(actions);
    overlay.appendChild(dialog);
    document.body.appendChild(overlay);
    confirmButton.focus();
  });
}
