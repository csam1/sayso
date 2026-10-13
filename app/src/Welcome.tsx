import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import {
  checkAccessibilityPermission,
  checkMicrophonePermission,
  requestAccessibilityPermission,
  requestMicrophonePermission,
} from "tauri-plugin-macos-permissions-api";
import "./Welcome.css";

const STEPS = [
  {
    title: "Click into any text field",
    body: "Terminal, Notes, Slack, a browser address bar — wherever you'd type.",
  },
  {
    title: "Press ⌘⇧Space and speak",
    body: "A small overlay appears. The bars move with your voice.",
  },
  {
    title: "Press ⌘⇧Space again (or click ■)",
    body: "Your speech is transcribed on this Mac. No audio leaves your machine.",
  },
  {
    title: "It's pasted where your cursor was",
    body: "It's also on your clipboard, so ⌘V works if the paste didn't land.",
  },
];

type PermissionState = boolean | null;

type PermissionRowProps = {
  name: string;
  why: string;
  granted: PermissionState;
  onGrant: () => void;
};

function PermissionRow({ name, why, granted, onGrant }: PermissionRowProps) {
  return (
    <li className="permission">
      <div>
        <div className="permission-name">{name}</div>
        <div className="permission-why">{why}</div>
      </div>
      {granted === null ? (
        <span className="permission-status">Checking…</span>
      ) : granted ? (
        <span className="permission-status permission-status--ok">✓ Granted</span>
      ) : (
        <div className="permission-actions">
          <span className="permission-status permission-status--missing">✗ Not granted</span>
          <button type="button" className="grant-button" onClick={onGrant}>
            Grant
          </button>
        </div>
      )}
    </li>
  );
}

function Welcome() {
  const [microphone, setMicrophone] = useState<PermissionState>(null);
  const [accessibility, setAccessibility] = useState<PermissionState>(null);

  const refresh = useCallback(async () => {
    try {
      const [mic, ax] = await Promise.all([
        checkMicrophonePermission(),
        checkAccessibilityPermission(),
      ]);
      setMicrophone(mic);
      setAccessibility(ax);
    } catch (error) {
      console.error("Permission check failed:", error);
    }
  }, []);

  useEffect(() => {
    refresh();
    // People usually leave for System Settings and come back, so re-check on focus.
    const unlisten = getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (focused) refresh();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [refresh]);

  const grant = async (request: () => Promise<unknown>) => {
    try {
      await request();
    } catch (error) {
      console.error("Permission request failed:", error);
    }
    refresh();
  };

  return (
    <main className="welcome">
      <header>
        <h1>Welcome to Sayso</h1>
        <p className="subtitle">Speak instead of typing — in any app.</p>
      </header>

      <ol className="steps">
        {STEPS.map((step, index) => (
          <li key={step.title} className="step">
            <span className="step-number">{index + 1}</span>
            <div>
              <div className="step-title">{step.title}</div>
              <div className="step-body">{step.body}</div>
            </div>
          </li>
        ))}
      </ol>

      <section>
        <h2>Permissions</h2>
        <ul className="permissions">
          <PermissionRow
            name="Microphone"
            why="To hear you."
            granted={microphone}
            onGrant={() => grant(requestMicrophonePermission)}
          />
          <PermissionRow
            name="Accessibility"
            why="To paste into other apps."
            granted={accessibility}
            onGrant={() => grant(requestAccessibilityPermission)}
          />
        </ul>
        <p className="footnote">
          Running from a terminal (<code>npm run tauri dev</code>)? macOS grants
          these to that terminal app, and you need to restart it after granting.
        </p>
      </section>

      <footer>
        <button
          type="button"
          className="done-button"
          onClick={() => invoke("welcome_done")}
        >
          Got it
        </button>
      </footer>
    </main>
  );
}

export default Welcome;
