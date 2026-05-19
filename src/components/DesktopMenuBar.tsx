import { useEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";

type MenuLabelSet = {
  file: string;
  edit: string;
  view: string;
  window: string;
  help: string;
  addFiles: string;
  createBundle: string;
  copyCode: string;
  reset: string;
  clearStream: string;
  pinWindow: string;
  unpinWindow: string;
  showUpdatePreview: string;
  hideUpdatePreview: string;
  minimizeWindow: string;
  maximizeWindow: string;
  restoreWindow: string;
  closeWindow: string;
  openWebsite: string;
  openDownloads: string;
  quitDropply: string;
};

type DesktopMenuBarProps = {
  title: string;
  labels: MenuLabelSet;
  isPinned: boolean;
  isMaximized: boolean;
  updatePreviewVisible: boolean;
  onStartDrag: () => Promise<void> | void;
  onOpenFiles: () => Promise<void> | void;
  onCreateBundle: () => Promise<void> | void;
  onCopyCode: () => Promise<void> | void;
  onResetPairing: () => Promise<void> | void;
  onClearStream: () => Promise<void> | void;
  onTogglePin: () => Promise<void> | void;
  onToggleUpdatePreview: () => Promise<void> | void;
  onOpenWebsite: () => Promise<void> | void;
  onOpenDownloads: () => Promise<void> | void;
  onMinimize: () => Promise<void> | void;
  onToggleMaximize: () => Promise<void> | void;
  onClose: () => Promise<void> | void;
};

type MenuKey = "file" | "edit" | "view" | "window" | "help";

export function DesktopMenuBar({
  title,
  labels,
  isPinned,
  isMaximized,
  updatePreviewVisible,
  onStartDrag,
  onOpenFiles,
  onCreateBundle,
  onCopyCode,
  onResetPairing,
  onClearStream,
  onTogglePin,
  onToggleUpdatePreview,
  onOpenWebsite,
  onOpenDownloads,
  onMinimize,
  onToggleMaximize,
  onClose,
}: DesktopMenuBarProps) {
  const [openMenu, setOpenMenu] = useState<MenuKey | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);

  function targetsInteractiveControl(target: EventTarget | null) {
    return target instanceof HTMLElement && Boolean(target.closest("[data-desktop-interactive='true']"));
  }

  useEffect(() => {
    function handlePointerDown(event: MouseEvent) {
      if (!rootRef.current?.contains(event.target as Node)) {
        setOpenMenu(null);
      }
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setOpenMenu(null);
      }
    }

    window.addEventListener("mousedown", handlePointerDown);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("mousedown", handlePointerDown);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, []);

  async function run(action: () => Promise<void> | void) {
    setOpenMenu(null);
    await action();
  }

  function handleChromeMouseDown(event: ReactMouseEvent<HTMLDivElement>) {
    if (event.button !== 0 || event.detail > 1 || targetsInteractiveControl(event.target)) {
      return;
    }

    void onStartDrag();
  }

  function handleChromeDoubleClick(event: ReactMouseEvent<HTMLDivElement>) {
    if (event.button !== 0 || targetsInteractiveControl(event.target)) {
      return;
    }

    void onToggleMaximize();
  }

  const menus: Array<{
    key: MenuKey;
    label: string;
    items: Array<{ label: string; action: () => Promise<void> | void; destructive?: boolean }>;
  }> = [
    {
      key: "file",
      label: labels.file,
      items: [
        { label: labels.addFiles, action: onOpenFiles },
        { label: labels.createBundle, action: onCreateBundle },
        { label: labels.openDownloads, action: onOpenDownloads },
        { label: labels.quitDropply, action: onClose, destructive: true },
      ],
    },
    {
      key: "edit",
      label: labels.edit,
      items: [
        { label: labels.copyCode, action: onCopyCode },
        { label: labels.reset, action: onResetPairing },
        { label: labels.clearStream, action: onClearStream, destructive: true },
      ],
    },
    {
      key: "view",
      label: labels.view,
      items: [
        { label: isPinned ? labels.unpinWindow : labels.pinWindow, action: onTogglePin },
        {
          label: updatePreviewVisible ? labels.hideUpdatePreview : labels.showUpdatePreview,
          action: onToggleUpdatePreview,
        },
      ],
    },
    {
      key: "window",
      label: labels.window,
      items: [
        { label: labels.minimizeWindow, action: onMinimize },
        { label: isMaximized ? labels.restoreWindow : labels.maximizeWindow, action: onToggleMaximize },
        { label: labels.closeWindow, action: onClose, destructive: true },
      ],
    },
    {
      key: "help",
      label: labels.help,
      items: [
        { label: labels.openWebsite, action: onOpenWebsite },
        { label: labels.openDownloads, action: onOpenDownloads },
      ],
    },
  ];

  return (
    <div
      className="desktop-menu-bar"
      ref={rootRef}
      onMouseDown={handleChromeMouseDown}
      onDoubleClick={handleChromeDoubleClick}
    >
      <div className="desktop-menu-bar__left">
        <span className="desktop-menu-bar__title">{title}</span>
        <div className="desktop-menu-bar__menus">
          {menus.map((menu) => (
            <div className="desktop-menu-bar__menu" key={menu.key}>
              <button
                type="button"
                className={`desktop-menu-bar__menu-button ${
                  openMenu === menu.key ? "desktop-menu-bar__menu-button--active" : ""
                }`}
                data-desktop-interactive="true"
                onClick={() => setOpenMenu((current) => (current === menu.key ? null : menu.key))}
              >
                {menu.label}
              </button>
              {openMenu === menu.key ? (
                <div className="desktop-menu-bar__dropdown">
                  {menu.items.map((item) => (
                    <button
                      type="button"
                      key={item.label}
                      className={`desktop-menu-bar__dropdown-item ${
                        item.destructive ? "desktop-menu-bar__dropdown-item--destructive" : ""
                      }`}
                      data-desktop-interactive="true"
                      onClick={() => void run(item.action)}
                    >
                      {item.label}
                    </button>
                  ))}
                </div>
              ) : null}
            </div>
          ))}
        </div>
      </div>

      <div className="desktop-menu-bar__drag-region" />

      <div className="desktop-menu-bar__controls">
        <button
          type="button"
          className="desktop-menu-bar__control"
          data-desktop-interactive="true"
          aria-label={labels.minimizeWindow}
          title={labels.minimizeWindow}
          onClick={() => void onMinimize()}
        >
          <span className="desktop-menu-bar__glyph desktop-menu-bar__glyph--minimize" aria-hidden="true" />
        </button>
        <button
          type="button"
          className="desktop-menu-bar__control"
          data-desktop-interactive="true"
          aria-label={isMaximized ? labels.restoreWindow : labels.maximizeWindow}
          title={isMaximized ? labels.restoreWindow : labels.maximizeWindow}
          onClick={() => void onToggleMaximize()}
        >
          <span
            className={`desktop-menu-bar__glyph ${
              isMaximized ? "desktop-menu-bar__glyph--restore" : "desktop-menu-bar__glyph--maximize"
            }`}
            aria-hidden="true"
          />
        </button>
        <button
          type="button"
          className="desktop-menu-bar__control desktop-menu-bar__control--close"
          data-desktop-interactive="true"
          aria-label={labels.closeWindow}
          title={labels.closeWindow}
          onClick={() => void onClose()}
        >
          <span className="desktop-menu-bar__glyph desktop-menu-bar__glyph--close" aria-hidden="true" />
        </button>
      </div>
    </div>
  );
}
