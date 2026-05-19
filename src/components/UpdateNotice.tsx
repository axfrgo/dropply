type UpdateNoticeProps = {
  availableVersion: string;
  currentVersion: string | null;
  onOpen: () => Promise<void> | void;
  title: string;
  subtitle: string;
  ctaLabel: string;
};

export function UpdateNotice({
  availableVersion,
  currentVersion,
  onOpen,
  title,
  subtitle,
  ctaLabel,
}: UpdateNoticeProps) {
  return (
    <button
      type="button"
      className="update-pill"
      onClick={() => void onOpen()}
      aria-label={`${title} ${availableVersion}`}
      title={
        currentVersion
          ? `${title} ${availableVersion} (${currentVersion} installed)`
          : `${title} ${availableVersion}`
      }
    >
      <span className="update-pill__icon" aria-hidden="true">
        <span className="update-pill__pulse" />
        <svg viewBox="0 0 20 20" className="update-pill__glyph" focusable="false">
          <path
            d="M10 3.25a.75.75 0 0 1 .75.75v6.19l1.97-1.97a.75.75 0 1 1 1.06 1.06l-3.25 3.25a.75.75 0 0 1-1.06 0L6.22 9.28a.75.75 0 1 1 1.06-1.06l1.97 1.97V4a.75.75 0 0 1 .75-.75ZM5.25 14a.75.75 0 0 1 .75.75v.5h8v-.5a.75.75 0 0 1 1.5 0V16A.75.75 0 0 1 14.75 17h-9.5A.75.75 0 0 1 4.5 16v-1.25a.75.75 0 0 1 .75-.75Z"
            fill="currentColor"
          />
        </svg>
      </span>
      <span className="update-pill__copy">
        <strong>{title}</strong>
        <span>{subtitle}</span>
      </span>
      <span className="update-pill__version">{ctaLabel}</span>
    </button>
  );
}
