import { createContext, useContext, useEffect, useMemo, type ReactNode } from "react";
import { localeStore, type Locale, useLocaleStore } from "./preferences";

const MESSAGES = {
  en: {
    brandSubtitle: "Smart Drops for everything in motion",
    localOnly: "local only",
    syncNotLive: "sync not live",
    linked: "{count} linked",
    pending: "{count} pending",
    items: "{count} items",
    pinned: "pinned",
    updateReadyTitle: "Update ready",
    updateReadySubtitle: "Dropply {version} is waiting.",
    downloadUpdate: "Install",
    updatePreviewEnabled: "Update preview enabled for {version}.",
    updatePreviewCleared: "Update preview cleared.",
    minimizeWindow: "Minimize window",
    maximizeWindow: "Maximize window",
    restoreWindow: "Restore window",
    closeWindow: "Close window",
    menuFile: "File",
    menuEdit: "Edit",
    menuView: "View",
    menuWindow: "Window",
    menuHelp: "Help",
    quitDropply: "Quit Dropply",
    showUpdatePreview: "Show update preview",
    hideUpdatePreview: "Hide update preview",
    openWebsite: "Open Dropply.ca",
    openDownloads: "Open Downloads",
    signIn: "Sign in",
    signUp: "Sign up",
    pinWindow: "Pin window",
    unpinWindow: "Unpin window",
    loadingScratchpad: "Loading your Smart Drops...",
    pairDevices: "Pair Devices",
    connectedDevices: "Connected Devices",
    scanToConnect: "Scan to connect another Dropply device.",
    relayMedia: "Relay Media",
    directMedia: "Direct Media",
    reset: "Reset",
    unpairDesktop: "Unpair this desktop",
    joinSession: "Join session",
    cancel: "Cancel",
    copyCode: "Copy code",
    copied: "Copied",
    pairCopyRelay:
      "Scan the code or copy the token to connect another device. Media is locked to relay mode until you switch it back.",
    pairCopyDirect:
      "Scan the code or copy the token to connect another device. Media is locked to the direct path while this mode is active.",
    pairCopyIdle: "Scan the code or copy the token to pair another device with this desktop.",
    pastePairingToken: "Paste pairing token here...",
    join: "Join",
    activeNow: "active now",
    minutesAgo: "{count}m ago",
    hoursAgo: "{count}h ago",
    daysAgo: "{count}d ago",
    thisDesktop: "{label} (this desktop)",
    current: "current",
    remove: "Remove",
    qrAlt: "QR code containing the Dropply pairing code",
    localFirstCanvas: "Local-first shared canvas",
    heroTitle: "Drop anything. It shows up everywhere.",
    heroCopy:
      "Keep text, files, screenshots, and quick thoughts in one live stream without accounts or extra friction.",
    composerPlaceholder: "Type or paste text here. Ctrl+Enter sends it to the stream.",
    addFiles: "Add files",
    paste: "Paste",
    clearStream: "Clear stream",
    sendToStream: "Send to stream",
    typingHint: "Text lands in the shared stream immediately after send.",
    idleHint: "Paste anywhere or start typing here.",
    emptyTitle: "Nothing here yet.",
    emptyCopy: "Drop a file, paste a screenshot, or send text from the composer above.",
    importing: "Importing...",
    copy: "Copy",
    download: "Download",
    delete: "Delete",
    smartDrop: "Smart Drop",
    smartSuggestions: "Smart Drop suggestions",
    smartActionUnavailable: "Coming soon as Dropply routing gets smarter.",
    openItem: "Open",
    sourceUnknown: "source unknown",
    sourceComposer: "composer",
    sourcePaste: "paste",
    sourceDragDrop: "drag drop",
    sourceFilePicker: "file picker",
    sourceBrowser: "browser",
    sourceRelay: "relay",
    sourceDirect: "direct",
    intentCaptured: "captured",
    intentPending: "pending",
    intentSent: "sent",
    intentResumed: "resumed",
    intentCompleted: "done",
    intentRevoked: "revoked",
    resumeLater: "Later",
    summarizeLater: "Summarize",
    sendToDevice: "Device",
    markDone: "Done",
    revoke: "Revoke",
    intentUpdateFailed: "Smart Drop update failed.",
    openItemFailed: "Failed to open that item.",
    droppedImageAlt: "Dropped image",
    fileFallback: "File",
    cloudTitleSignup: "Create your cloud space",
    cloudTitleSignin: "Sign in to Dropply",
    cloudSubtitleSignup:
      "Hosted auth unlocks web, mobile, and cloud sync without changing the local-first desktop flow.",
    cloudSubtitleSignin: "Continue into Dropply Cloud for Google, email-link, and passkey sign-in.",
    cloudLabel: "Dropply Cloud",
    welcomeBack: "Welcome back",
    close: "Close",
    backendOnline: "Backend online",
    backendOffline: "Backend offline",
    openSignUp: "Open sign up",
    openSignIn: "Open sign in",
    useHostedAuth: "Use hosted auth",
    availableMethods: "Available methods",
    methodsFallback: "Google, email magic link, and passkeys.",
    localNoLogin: "Local mode needs no login. Sign in only for hosted sync.",
    hostedAuthMissing:
      "Hosted auth is not configured right now. Dropply desktop remains fully local-first.",
    hostedAuthDisabled: "Hosted auth is not configured right now.",
    openedHostedAuth: "Opened Dropply Cloud {label} in your browser.",
    hostedSyncUsesSignin: "Hosted sync uses sign-in.",
    cloudSigninOptional: "Cloud sign-in is optional.",
    cloudManagedByClerk: "Clerk manages the hosted account flow.",
    savedToStream: "Saved to stream",
    copiedToClipboard: "Copied to clipboard",
    savedToDownloads: "Saved to Downloads",
    failedToBoot: "Failed to boot Dropply.",
    textImportFailed: "Text import failed.",
    fileImportFailed: "File import failed.",
    failedToCopyText: "Failed to copy text.",
    deleteFailed: "Delete failed.",
    bulkDeleteFailed: "Bulk delete failed.",
    downloadFailed: "Download failed.",
    failedToUpdateToken: "Failed to update token.",
    failedToResetToken: "Failed to reset token.",
    failedToUnpair: "Failed to unpair.",
    failedToRemovePairedDevice: "Failed to remove paired device.",
    directDesktopLinkUnavailable: "Unable to open the direct desktop media link.",
    relayBlobUploadFailed: "Relay blob upload failed.",
    relayPushFailed: "Relay push failed.",
    relayUploadResuming: "Resuming {label} from chunk {current}/{total}.",
    relayUploadProgress: "Uploading {label}: {current}/{total} chunks ({percent}%)",
    relayUploadRetrying:
      "Retrying {label} after a network hiccup ({attempt}/{totalAttempts}).",
    relayUploadComplete: "{label} finished uploading through relay.",
    directManifestRefreshFailed: "Failed to refresh the direct transfer manifest.",
    unpairMessage:
      "Are you sure you want to unpair this device? All local data will be cleared and the app will restart.",
    unpairTitle: "Unpair device",
    unpairConfirm: "Unpair",
    removeDeviceMessage:
      "Remove {label} from this pairing? That device will stop syncing with this code until you reset the pairing token or re-authorize it.",
    removeDeviceTitle: "Remove paired device",
    removeDeviceConfirm: "Remove",
    clearStreamMessage:
      "Are you sure you want to clear the entire stream? This will affect all linked devices.",
    clearStreamConfirm: "Clear stream",
    createBundle: "Create bundle",
    itemTypeText: "text",
    itemTypeImage: "image",
    itemTypeFile: "file",
    conversationBundle: "Smart Drop bundle",
    viewBundle: "View bundle",
    bundleLoading: "Opening Smart Drop bundle...",
    bundleLoadingEntry: "Loading bundle entry...",
    bundleOpenFailed: "Failed to open the Smart Drop bundle.",
    bundleEntryLoadFailed: "Failed to load this bundle entry.",
    bundleRejectedTitle: "Sandbox rejected this bundle",
    bundleRejectedHint: "Dropply blocked this bundle before previewing it. Review the exact reason below.",
    bundleRejectedReasonLabel: "Reason",
    bundleTranscript: "Transcript",
    bundleTranscriptHint: "Markdown transcript captured at send time.",
    bundleFiles: "Referenced files",
    bundleAttachments: "Attachments",
    bundleFilesCount: "{count} files",
    bundleAttachmentsCount: "{count} attachments",
    bundleNoFiles: "No referenced files were included.",
    bundleNoAttachments: "No attachments were included.",
    bundlePreviewUnavailable: "Preview is unavailable for this file type. Download the bundle to inspect it fully.",
    bundleTileHint: "Open the bundle to review the transcript, files, and attachments together.",
    bundleSavedToStream: "Smart Drop bundle saved to stream",
    bundleCreateFailed: "Failed to create the Smart Drop bundle.",
    bundleComposeHint:
      "Package one transcript, the files we referenced, and any supporting docs into a single Dropply bundle.",
    bundleTitleLabel: "Bundle title",
    bundleTitlePlaceholder: "Research session review",
    bundleSourceLabel: "Source app",
    bundleSourcePlaceholder: "Browser, docs, VS Code...",
    bundleSourceUrlLabel: "Source URL (optional)",
    bundleSourceUrlPlaceholder: "https://...",
    bundleLoadTranscript: "Load transcript file",
    bundleTranscriptPlaceholder: "Paste the conversation transcript here in Markdown or plain text.",
    bundleTranscriptEditorHint: "{count} non-empty lines ready for the bundle.",
    bundleTranscriptLoadFailed: "Failed to load the transcript file.",
    bundleTranscriptRequired: "Add a transcript before sending the bundle.",
    bundleFilesHint: "These stay grouped with the transcript as referenced source files.",
    bundleAttachmentsHint: "Use attachments for summaries, screenshots, or extra markdown docs.",
    bundleAddFiles: "Add files",
    bundleAddAttachments: "Add attachments",
    bundleFilePickFailed: "Failed to add those files to the bundle.",
    bundleSaving: "Saving bundle...",
    bundleSend: "Send bundle",
    deviceTypeDesktop: "desktop",
    deviceTypeWeb: "browser",
    deviceTypeMobile: "phone",
  },
  fr: {
    brandSubtitle: "Smart Drops pour tout ce qui bouge",
    localOnly: "local seulement",
    syncNotLive: "sync inactive",
    linked: "{count} lie(s)",
    pending: "{count} en attente",
    items: "{count} elements",
    pinned: "epingle",
    updateReadyTitle: "Mise a jour prete",
    updateReadySubtitle: "Dropply {version} vous attend.",
    downloadUpdate: "Installer",
    updatePreviewEnabled: "Apercu de mise a jour active pour {version}.",
    updatePreviewCleared: "Apercu de mise a jour efface.",
    minimizeWindow: "Reduire la fenetre",
    maximizeWindow: "Agrandir la fenetre",
    restoreWindow: "Restaurer la fenetre",
    closeWindow: "Fermer la fenetre",
    menuFile: "Fichier",
    menuEdit: "Edition",
    menuView: "Affichage",
    menuWindow: "Fenetre",
    menuHelp: "Aide",
    quitDropply: "Quitter Dropply",
    showUpdatePreview: "Afficher l'aperçu de mise a jour",
    hideUpdatePreview: "Masquer l'aperçu de mise a jour",
    openWebsite: "Ouvrir Dropply.ca",
    openDownloads: "Ouvrir Telechargements",
    signIn: "Se connecter",
    signUp: "Creer un compte",
    pinWindow: "Epingler la fenetre",
    unpinWindow: "Desepingler la fenetre",
    loadingScratchpad: "Chargement de vos Smart Drops...",
    pairDevices: "Associer des appareils",
    connectedDevices: "Appareils connectes",
    scanToConnect: "Scannez pour connecter un autre appareil Dropply.",
    relayMedia: "Media relais",
    directMedia: "Media direct",
    reset: "Reinitialiser",
    unpairDesktop: "Desassocier ce bureau",
    joinSession: "Rejoindre la session",
    cancel: "Annuler",
    copyCode: "Copier le code",
    copied: "Copie",
    pairCopyRelay:
      "Scannez le code ou copiez le jeton pour connecter un autre appareil. Les medias restent forces au mode relais jusqu'a ce que vous changiez ce mode.",
    pairCopyDirect:
      "Scannez le code ou copiez le jeton pour connecter un autre appareil. Les medias restent forces au lien direct tant que ce mode est actif.",
    pairCopyIdle:
      "Scannez le code ou copiez le jeton pour associer un autre appareil a ce bureau.",
    pastePairingToken: "Collez le jeton d'association ici...",
    join: "Joindre",
    activeNow: "actif maintenant",
    minutesAgo: "il y a {count} min",
    hoursAgo: "il y a {count} h",
    daysAgo: "il y a {count} j",
    thisDesktop: "{label} (ce bureau)",
    current: "actuel",
    remove: "Retirer",
    qrAlt: "Code QR contenant le code d'association Dropply",
    localFirstCanvas: "Canvas local-first partage",
    heroTitle: "Deposez n'importe quoi. Ca apparait partout.",
    heroCopy:
      "Gardez textes, fichiers, captures et idees rapides dans un flux unique sans compte obligatoire ni friction.",
    composerPlaceholder:
      "Tapez ou collez du texte ici. Ctrl+Entree l'envoie dans le flux.",
    addFiles: "Ajouter des fichiers",
    paste: "Coller",
    clearStream: "Vider le flux",
    sendToStream: "Envoyer au flux",
    typingHint: "Le texte arrive dans le flux partage juste apres l'envoi.",
    idleHint: "Collez n'importe ou ou commencez a taper ici.",
    emptyTitle: "Rien ici pour l'instant.",
    emptyCopy: "Deposez un fichier, collez une capture ou envoyez du texte depuis le composeur.",
    importing: "Importation...",
    copy: "Copier",
    download: "Telecharger",
    delete: "Supprimer",
    smartDrop: "Smart Drop",
    smartSuggestions: "Suggestions Smart Drop",
    smartActionUnavailable: "Bientot disponible avec le routage Dropply.",
    openItem: "Ouvrir",
    sourceUnknown: "source inconnue",
    sourceComposer: "composeur",
    sourcePaste: "collage",
    sourceDragDrop: "glisser",
    sourceFilePicker: "fichier",
    sourceBrowser: "navigateur",
    sourceRelay: "relais",
    sourceDirect: "direct",
    intentCaptured: "capture",
    intentPending: "en attente",
    intentSent: "envoye",
    intentResumed: "repris",
    intentCompleted: "termine",
    intentRevoked: "revoque",
    resumeLater: "Plus tard",
    summarizeLater: "Resumer",
    sendToDevice: "Appareil",
    markDone: "Termine",
    revoke: "Revoquer",
    intentUpdateFailed: "Impossible de mettre a jour le Smart Drop.",
    openItemFailed: "Impossible d'ouvrir cet element.",
    droppedImageAlt: "Image deposee",
    fileFallback: "Fichier",
    cloudTitleSignup: "Creez votre espace cloud",
    cloudTitleSignin: "Se connecter a Dropply",
    cloudSubtitleSignup:
      "L'authentification hebergee active le web, le mobile et la sync cloud sans changer le flux local-first du bureau.",
    cloudSubtitleSignin:
      "Continuez vers Dropply Cloud pour Google, le lien courriel et les passkeys.",
    cloudLabel: "Dropply Cloud",
    welcomeBack: "Bon retour",
    close: "Fermer",
    backendOnline: "Backend en ligne",
    backendOffline: "Backend hors ligne",
    openSignUp: "Ouvrir l'inscription",
    openSignIn: "Ouvrir la connexion",
    useHostedAuth: "Utiliser l'auth hebergee",
    availableMethods: "Methodes disponibles",
    methodsFallback: "Google, lien magique courriel et passkeys.",
    localNoLogin:
      "Le mode local n'a pas besoin de connexion. Connectez-vous seulement pour la sync hebergee.",
    hostedAuthMissing:
      "L'authentification hebergee n'est pas configuree pour le moment. Dropply bureau reste entierement local-first.",
    hostedAuthDisabled: "L'authentification hebergee n'est pas configuree pour le moment.",
    openedHostedAuth: "Dropply Cloud {label} a ete ouvert dans votre navigateur.",
    hostedSyncUsesSignin: "La sync hebergee utilise la connexion.",
    cloudSigninOptional: "La connexion cloud est facultative.",
    cloudManagedByClerk: "Clerk gere le flux d'authentification heberge.",
    savedToStream: "Enregistre dans le flux",
    copiedToClipboard: "Copie dans le presse-papiers",
    savedToDownloads: "Enregistre dans Telechargements",
    failedToBoot: "Echec du demarrage de Dropply.",
    textImportFailed: "Echec de l'import du texte.",
    fileImportFailed: "Echec de l'import du fichier.",
    failedToCopyText: "Impossible de copier le texte.",
    deleteFailed: "Echec de la suppression.",
    bulkDeleteFailed: "Echec de la suppression globale.",
    downloadFailed: "Echec du telechargement.",
    failedToUpdateToken: "Impossible de mettre a jour le jeton.",
    failedToResetToken: "Impossible de reinitialiser le jeton.",
    failedToUnpair: "Impossible de desassocier.",
    failedToRemovePairedDevice: "Impossible de retirer l'appareil associe.",
    directDesktopLinkUnavailable: "Impossible d'ouvrir le lien media direct du bureau.",
    relayBlobUploadFailed: "Echec de l'envoi du blob relais.",
    relayPushFailed: "Echec de l'envoi relais.",
    relayUploadResuming:
      "Reprise de {label} a partir du bloc {current}/{total}.",
    relayUploadProgress:
      "Televersement de {label} : {current}/{total} blocs ({percent} %)",
    relayUploadRetrying:
      "Nouvel essai pour {label} apres un souci reseau ({attempt}/{totalAttempts}).",
    relayUploadComplete: "{label} a termine son envoi par relais.",
    directManifestRefreshFailed:
      "Impossible d'actualiser le manifeste de transfert direct.",
    unpairMessage:
      "Voulez-vous vraiment desassocier cet appareil? Toutes les donnees locales seront effacees et l'application redemarrera.",
    unpairTitle: "Desassocier l'appareil",
    unpairConfirm: "Desassocier",
    removeDeviceMessage:
      "Retirer {label} de cette association? Cet appareil cessera de se synchroniser avec ce code jusqu'a la reinitialisation du jeton ou une nouvelle autorisation.",
    removeDeviceTitle: "Retirer l'appareil associe",
    removeDeviceConfirm: "Retirer",
    clearStreamMessage:
      "Voulez-vous vraiment vider tout le flux? Cela affectera tous les appareils lies.",
    clearStreamConfirm: "Vider le flux",
    createBundle: "Creer un bundle",
    itemTypeText: "texte",
    itemTypeImage: "image",
    itemTypeFile: "fichier",
    conversationBundle: "bundle Smart Drop",
    viewBundle: "Ouvrir le bundle",
    bundleLoading: "Ouverture du bundle Smart Drop...",
    bundleLoadingEntry: "Chargement de l'entree du bundle...",
    bundleOpenFailed: "Impossible d'ouvrir le bundle Smart Drop.",
    bundleEntryLoadFailed: "Impossible de charger cette entree du bundle.",
    bundleRejectedTitle: "Le sandbox a refuse ce bundle",
    bundleRejectedHint:
      "Dropply a bloque ce bundle avant l'aperçu. Consultez la raison exacte ci-dessous.",
    bundleRejectedReasonLabel: "Raison",
    bundleTranscript: "Transcription",
    bundleTranscriptHint: "Transcription Markdown capturee au moment de l'envoi.",
    bundleFiles: "Fichiers references",
    bundleAttachments: "Pieces jointes",
    bundleFilesCount: "{count} fichiers",
    bundleAttachmentsCount: "{count} pieces jointes",
    bundleNoFiles: "Aucun fichier reference n'a ete inclus.",
    bundleNoAttachments: "Aucune piece jointe n'a ete incluse.",
    bundlePreviewUnavailable:
      "Apercu indisponible pour ce type de fichier. Telechargez le bundle pour l'inspecter completement.",
    bundleTileHint:
      "Ouvrez le bundle pour revoir ensemble la transcription, les fichiers et les pieces jointes.",
    bundleSavedToStream: "Bundle Smart Drop enregistre dans le flux",
    bundleCreateFailed: "Impossible de creer le bundle Smart Drop.",
    bundleComposeHint:
      "Regroupez une transcription, les fichiers references et les documents utiles dans un seul bundle Dropply.",
    bundleTitleLabel: "Titre du bundle",
    bundleTitlePlaceholder: "Revue de session de recherche",
    bundleSourceLabel: "Application source",
    bundleSourcePlaceholder: "Navigateur, docs, VS Code...",
    bundleSourceUrlLabel: "URL source (optionnelle)",
    bundleSourceUrlPlaceholder: "https://...",
    bundleLoadTranscript: "Charger un fichier de transcription",
    bundleTranscriptPlaceholder: "Collez ici la transcription en Markdown ou en texte brut.",
    bundleTranscriptEditorHint: "{count} lignes non vides pretes pour le bundle.",
    bundleTranscriptLoadFailed: "Impossible de charger le fichier de transcription.",
    bundleTranscriptRequired: "Ajoutez une transcription avant d'envoyer le bundle.",
    bundleFilesHint: "Ces fichiers restent groupes avec la transcription comme sources referencees.",
    bundleAttachmentsHint: "Utilisez les pieces jointes pour les resumes, captures ou documents Markdown.",
    bundleAddFiles: "Ajouter des fichiers",
    bundleAddAttachments: "Ajouter des pieces jointes",
    bundleFilePickFailed: "Impossible d'ajouter ces fichiers au bundle.",
    bundleSaving: "Enregistrement du bundle...",
    bundleSend: "Envoyer le bundle",
    deviceTypeDesktop: "bureau",
    deviceTypeWeb: "navigateur",
    deviceTypeMobile: "telephone",
  },
} as const;

type MessageKey = keyof (typeof MESSAGES)["en"];

type I18nContextValue = {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  t: (key: MessageKey, vars?: Record<string, string | number>) => string;
  formatDateTime: (value: string | number | Date) => string;
  formatBytes: (size: number) => string;
  formatRelativeTime: (timestamp: number) => string;
  formatItemType: (value: "text" | "image" | "file") => string;
  formatDeviceType: (value: "desktop" | "web" | "mobile") => string;
};

const I18nContext = createContext<I18nContextValue | null>(null);

function localeTag(locale: Locale) {
  return locale === "fr" ? "fr-CA" : "en-US";
}

function interpolate(template: string, vars?: Record<string, string | number>) {
  if (!vars) {
    return template;
  }

  return template.replace(/\{(\w+)\}/g, (_, key: string) => String(vars[key] ?? ""));
}

export function I18nProvider({ children }: { children: ReactNode }) {
  const locale = useLocaleStore();

  useEffect(() => {
    if (typeof window !== "undefined") {
      document.documentElement.lang = locale;
    }
  }, [locale]);

  const value = useMemo<I18nContextValue>(() => {
    const current = MESSAGES[locale];
    const t = (key: MessageKey, vars?: Record<string, string | number>) =>
      interpolate(current[key] ?? MESSAGES.en[key], vars);

    const formatDateTime = (value: string | number | Date) =>
      new Intl.DateTimeFormat(localeTag(locale), {
        dateStyle: "medium",
        timeStyle: "short",
      }).format(new Date(value));

    const formatBytes = (size: number) => {
      if (size < 1024) {
        return `${size} B`;
      }
      if (size < 1024 * 1024) {
        return `${(size / 1024).toFixed(1)} ${locale === "fr" ? "Ko" : "KB"}`;
      }
      if (size < 1024 * 1024 * 1024) {
        return `${(size / (1024 * 1024)).toFixed(1)} ${locale === "fr" ? "Mo" : "MB"}`;
      }
      return `${(size / (1024 * 1024 * 1024)).toFixed(1)} ${locale === "fr" ? "Go" : "GB"}`;
    };

    const formatRelativeTime = (timestamp: number) => {
      const deltaMs = Date.now() - timestamp;
      if (deltaMs < 20_000) {
        return t("activeNow");
      }

      const minutes = Math.max(1, Math.round(deltaMs / 60_000));
      if (minutes < 60) {
        return t("minutesAgo", { count: minutes });
      }

      const hours = Math.round(minutes / 60);
      if (hours < 24) {
        return t("hoursAgo", { count: hours });
      }

      const days = Math.round(hours / 24);
      return t("daysAgo", { count: days });
    };

    const formatItemType = (value: "text" | "image" | "file") =>
      value === "text" ? t("itemTypeText") : value === "image" ? t("itemTypeImage") : t("itemTypeFile");

    const formatDeviceType = (value: "desktop" | "web" | "mobile") =>
      value === "desktop"
        ? t("deviceTypeDesktop")
        : value === "web"
          ? t("deviceTypeWeb")
          : t("deviceTypeMobile");

    return {
      locale,
      setLocale: localeStore.set,
      t,
      formatDateTime,
      formatBytes,
      formatRelativeTime,
      formatItemType,
      formatDeviceType,
    };
  }, [locale]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n() {
  const context = useContext(I18nContext);
  if (!context) {
    throw new Error("useI18n must be used within an I18nProvider.");
  }
  return context;
}
