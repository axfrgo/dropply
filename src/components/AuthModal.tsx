import { useEffect, useMemo, useState } from "react";
import {
  fetchCloudConfig,
  fetchCloudHealth,
  fetchCloudProviders,
  openHostedAuthPath,
  type CloudProvider,
} from "../lib/cloud";
import { useI18n } from "../lib/i18n";

type AuthModalProps = {
  mode: "signin" | "signup";
  onClose: () => void;
  onOpenExternalUrl: (url: string) => Promise<void>;
};

export function AuthModal({ mode, onClose, onOpenExternalUrl }: AuthModalProps) {
  const { t } = useI18n();
  const [isOnline, setIsOnline] = useState(false);
  const [providers, setProviders] = useState<CloudProvider[]>([]);
  const [notice, setNotice] = useState(t("localNoLogin"));
  const [authReady, setAuthReady] = useState(false);
  const [isBusy, setIsBusy] = useState(false);

  const title = mode === "signup" ? t("cloudTitleSignup") : t("cloudTitleSignin");
  const subtitle = useMemo(
    () => (mode === "signup" ? t("cloudSubtitleSignup") : t("cloudSubtitleSignin")),
    [mode, t]
  );

  useEffect(() => {
    let isMounted = true;

    void Promise.allSettled([fetchCloudHealth(), fetchCloudProviders(), fetchCloudConfig()]).then(
      ([healthResult, providersResult, configResult]) => {
        if (!isMounted) {
          return;
        }

        if (healthResult.status === "fulfilled") {
          setIsOnline(healthResult.value);
        }

        if (providersResult.status === "fulfilled") {
          setProviders(providersResult.value);
        }

        if (configResult.status === "fulfilled") {
          const ready = Boolean(
            configResult.value.auth_configured && configResult.value.hosted_sync_available
          );
          setAuthReady(ready);
          setNotice(
            ready
              ? `${configResult.value.hosted_sync_requires_login ? t("hostedSyncUsesSignin") : t("cloudSigninOptional")} ${configResult.value.auth_provider === "clerk" ? t("cloudManagedByClerk") : ""}`.trim()
              : t("hostedAuthMissing")
          );
        }
      }
    );

    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        onClose();
      }
    }

    window.addEventListener("keydown", onKeyDown);

    return () => {
      isMounted = false;
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [onClose, t]);

  async function continueHosted(path: "/signin" | "/signup", label: string) {
    if (!authReady) {
      setNotice(t("hostedAuthDisabled"));
      return;
    }

    setIsBusy(true);
    try {
      const url = await openHostedAuthPath(path);
      await onOpenExternalUrl(url);
      setNotice(t("openedHostedAuth", { label }));
    } catch (error) {
      setNotice(error instanceof Error ? error.message : t("hostedAuthDisabled"));
    } finally {
      setIsBusy(false);
    }
  }

  const providerSummary = providers
    .filter((provider) => provider.enabled)
    .map((provider) => provider.label)
    .join(", ");

  return (
    <div className="auth-modal-backdrop" role="presentation" onClick={onClose}>
      <section
        className="auth-modal"
        aria-label={mode === "signup" ? t("cloudTitleSignup") : t("cloudTitleSignin")}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="auth-modal-header">
          <div>
            <p className="eyebrow">{mode === "signup" ? t("cloudLabel") : t("welcomeBack")}</p>
            <h2>{title}</h2>
            <p className="auth-modal-copy">{subtitle}</p>
          </div>
          <button type="button" className="composer-tool" onClick={onClose}>
            {t("close")}
          </button>
        </div>

        <div className="auth-modal-status-row">
          <span className={`cloud-status ${isOnline ? "is-online" : "is-offline"}`}>
            {isOnline ? t("backendOnline") : t("backendOffline")}
          </span>
          <p className="auth-modal-note">{notice}</p>
        </div>

        <div className="auth-modal-actions">
          <button
            type="button"
            className="composer-send"
            onClick={() =>
              void continueHosted(
                mode === "signup" ? "/signup" : "/signin",
                mode === "signup" ? "sign-up" : "sign-in"
              )
            }
            disabled={isBusy || !authReady}
          >
            {mode === "signup" ? t("openSignUp") : t("openSignIn")}
          </button>
          <button
            type="button"
            className="composer-tool"
            onClick={() => void continueHosted("/signin", "sign-in")}
            disabled={isBusy || !authReady}
          >
            {t("useHostedAuth")}
          </button>
        </div>

        <div className="auth-modal-provider-list">
          <span className="auth-modal-provider-label">{t("availableMethods")}</span>
          <p className="auth-modal-note">{providerSummary || t("methodsFallback")}</p>
        </div>
      </section>
    </div>
  );
}
