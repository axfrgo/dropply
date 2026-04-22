import { useEffect, useMemo, useState } from "react";
import {
  fetchCloudConfig,
  fetchCloudHealth,
  fetchCloudProviders,
  openHostedAuthPath,
  type CloudProvider,
} from "../lib/cloud";

type AuthModalProps = {
  mode: "signin" | "signup";
  onClose: () => void;
  onOpenExternalUrl: (url: string) => Promise<void>;
};

export function AuthModal({ mode, onClose, onOpenExternalUrl }: AuthModalProps) {
  const [isOnline, setIsOnline] = useState(false);
  const [providers, setProviders] = useState<CloudProvider[]>([]);
  const [notice, setNotice] = useState("Local mode needs no login. Sign in only for hosted sync.");
  const [isBusy, setIsBusy] = useState(false);

  const title = mode === "signup" ? "Create your cloud space" : "Sign in to Dropply";
  const subtitle = useMemo(
    () =>
      mode === "signup"
        ? "Clerk-managed hosted auth unlocks web, mobile, and cloud sync without changing the local-first desktop flow."
        : "Continue into Dropply Cloud for Google, email-link, and passkey sign-in.",
    [mode]
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
          setNotice(
            `${configResult.value.hosted_sync_requires_login ? "Hosted sync uses sign-in." : "Cloud sign-in is optional."} ${configResult.value.auth_provider === "clerk" ? "Clerk manages the hosted account flow." : ""}`.trim()
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
  }, [onClose]);

  async function continueHosted(path: "/signin" | "/signup", label: string) {
    setIsBusy(true);
    try {
      const url = await openHostedAuthPath(path);
      await onOpenExternalUrl(url);
      setNotice(`Opened Dropply Cloud ${label} in your browser.`);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "Hosted auth could not be opened.");
    } finally {
      setIsBusy(false);
    }
  }

  const providerSummary = providers.filter((provider) => provider.enabled).map((provider) => provider.label).join(", ");

  return (
    <div className="auth-modal-backdrop" role="presentation" onClick={onClose}>
      <section
        className="auth-modal"
        aria-label={mode === "signup" ? "Create a Dropply account" : "Sign in to Dropply"}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="auth-modal-header">
          <div>
            <p className="eyebrow">{mode === "signup" ? "Dropply Cloud" : "Welcome back"}</p>
            <h2>{title}</h2>
            <p className="auth-modal-copy">{subtitle}</p>
          </div>
          <button type="button" className="composer-tool" onClick={onClose}>
            Close
          </button>
        </div>

        <div className="auth-modal-status-row">
          <span className={`cloud-status ${isOnline ? "is-online" : "is-offline"}`}>
            {isOnline ? "Backend online" : "Backend offline"}
          </span>
          <p className="auth-modal-note">{notice}</p>
        </div>

        <div className="auth-modal-actions">
          <button
            type="button"
            className="composer-send"
            onClick={() => void continueHosted(mode === "signup" ? "/signup" : "/signin", mode === "signup" ? "sign-up" : "sign-in")}
            disabled={isBusy}
          >
            {mode === "signup" ? "Open sign up" : "Open sign in"}
          </button>
          <button
            type="button"
            className="composer-tool"
            onClick={() => void continueHosted("/signin", "sign-in")}
            disabled={isBusy}
          >
            Use hosted auth
          </button>
        </div>

        <div className="auth-modal-provider-list">
          <span className="auth-modal-provider-label">Available methods</span>
          <p className="auth-modal-note">{providerSummary || "Google, email magic link, and passkeys."}</p>
        </div>
      </section>
    </div>
  );
}
