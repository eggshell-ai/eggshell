import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChatMessage } from "./Chat";

export const FEEDBACK_REASONS = [
  "It didn't do what I asked",
  "The answer was incorrect",
  "I don't like the result",
  "Other",
] as const;

export type FeedbackReason = (typeof FEEDBACK_REASONS)[number];

type FeedbackPopupProps = {
  isOpen: boolean;
  onClose: () => void;
  activeProvider: string;
  activeModel: string;
  messages: ChatMessage[];
  targetMessageIndex: number;
  onSubmitted: () => void;
};

export default function FeedbackPopup({
  isOpen,
  onClose,
  activeProvider,
  activeModel,
  messages,
  targetMessageIndex,
  onSubmitted,
}: FeedbackPopupProps) {
  const [selectedReason, setSelectedReason] = useState<FeedbackReason>(FEEDBACK_REASONS[0]);
  const [notes, setNotes] = useState("");
  const [attachPromptHistory, setAttachPromptHistory] = useState(true);
  const [step, setStep] = useState<"form" | "review">("form");
  const [isSending, setIsSending] = useState(false);
  const [appVersion, setAppVersion] = useState("1.0.5");
  const [error, setError] = useState("");

  useEffect(() => {
    if (isOpen) {
      setSelectedReason(FEEDBACK_REASONS[0]);
      setNotes("");
      setAttachPromptHistory(true);
      setStep("form");
      setIsSending(false);
      setError("");

      invoke<string>("get_app_version")
        .then((ver) => setAppVersion(ver))
        .catch(() => {});
    }
  }, [isOpen]);

  if (!isOpen) return null;

  // Extract user messages and assistant responses up to the target response
  const relevantHistory = messages
    .slice(0, targetMessageIndex + 1)
    .filter((m) => m.role === "user" || m.role === "assistant");
  const userMsgCount = relevantHistory.filter((m) => m.role === "user").length;
  const assistantMsgCount = relevantHistory.filter((m) => m.role === "assistant").length;

  async function handleSend() {
    setIsSending(true);
    setError("");

    try {
      const promptHistory = attachPromptHistory
        ? relevantHistory.map((m) => ({
            role: m.role,
            content: m.content,
          }))
        : null;

      await invoke("submit_feedback", {
        payload: {
          rating: "thumbs_down",
          reason: selectedReason,
          notes: notes.trim() || null,
          attachPromptHistory,
          promptHistory,
          provider: activeProvider || null,
          model: activeModel || null,
        },
      });

      onSubmitted();
      onClose();
    } catch (err) {
      setError(`Failed to send feedback: ${String(err)}`);
      setIsSending(false);
    }
  }

  return (
    <div className="dialog-backdrop" role="presentation" onClick={onClose}>
      <section
        className="report-dialog feedback-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="feedback-dialog-title"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="dialog-heading">
          <div>
            <p className="eyebrow">Feedback</p>
            <h2 id="feedback-dialog-title">
              {step === "form" ? "What went wrong?" : "Review Report"}
            </h2>
          </div>
          <button
            className="icon-button"
            type="button"
            aria-label="Close"
            onClick={onClose}
            disabled={isSending}
          >
            ×
          </button>
        </div>

        {step === "form" ? (
          <div className="feedback-form-body">
            <fieldset className="feedback-options-group">
              <legend className="feedback-options-legend">Please select a reason:</legend>
              <div className="feedback-radio-list">
                {FEEDBACK_REASONS.map((reason) => (
                  <label key={reason} className="feedback-radio-label">
                    <input
                      type="radio"
                      name="feedback-reason"
                      value={reason}
                      checked={selectedReason === reason}
                      onChange={() => setSelectedReason(reason)}
                    />
                    <span>{reason}</span>
                  </label>
                ))}
              </div>
            </fieldset>

            <div className="feedback-notes-group">
              <label htmlFor="feedback-notes" className="feedback-notes-label">
                Notes:
              </label>
              <textarea
                id="feedback-notes"
                className="feedback-notes-input"
                placeholder="The user can enter any notes here"
                rows={3}
                value={notes}
                onChange={(e) => setNotes(e.target.value)}
              />
            </div>

            <label className="diagnostics-check feedback-history-check">
              <input
                type="checkbox"
                checked={attachPromptHistory}
                onChange={(e) => setAttachPromptHistory(e.target.checked)}
              />
              Attach prompt history
            </label>

            <div className="dialog-actions">
              <button className="secondary-button" type="button" onClick={onClose}>
                Cancel
              </button>
              <button
                className="add-button"
                type="button"
                onClick={() => setStep("review")}
              >
                Review
              </button>
            </div>
          </div>
        ) : (
          <div className="feedback-review-body">
            <p className="feedback-review-intro">This report contains</p>

            <ul className="feedback-manifest-list">
              <li className={`manifest-item ${attachPromptHistory ? "included" : "excluded"}`}>
                <span className="manifest-marker" aria-hidden="true">
                  {attachPromptHistory ? "✓" : "—"}
                </span>
                <span className="manifest-label">
                  Your messages
                  {attachPromptHistory ? (
                    <span className="manifest-sub"> ({userMsgCount} messages)</span>
                  ) : (
                    <span className="manifest-tag">Not sent</span>
                  )}
                </span>
              </li>

              <li className={`manifest-item ${attachPromptHistory ? "included" : "excluded"}`}>
                <span className="manifest-marker" aria-hidden="true">
                  {attachPromptHistory ? "✓" : "—"}
                </span>
                <span className="manifest-label">
                  Assistant responses
                  {attachPromptHistory ? (
                    <span className="manifest-sub"> ({assistantMsgCount} responses)</span>
                  ) : (
                    <span className="manifest-tag">Not sent</span>
                  )}
                </span>
              </li>

              <li className="manifest-item included">
                <span className="manifest-marker" aria-hidden="true">✓</span>
                <span className="manifest-label">
                  Model and provider
                  <span className="manifest-sub">
                    {" "}
                    ({activeModel || "Default model"} · {activeProvider || "Default provider"})
                  </span>
                </span>
              </li>

              <li className="manifest-item included">
                <span className="manifest-marker" aria-hidden="true">✓</span>
                <span className="manifest-label">
                  App version
                  <span className="manifest-sub"> (v{appVersion})</span>
                </span>
              </li>

              <li className={`manifest-item ${attachPromptHistory ? "included" : "excluded"}`}>
                <span className="manifest-marker" aria-hidden="true">
                  {attachPromptHistory ? "✓" : "—"}
                </span>
                <span className="manifest-label">
                  Diagnostic logs
                  {attachPromptHistory ? (
                    <span className="manifest-sub"> (Application and execution logs)</span>
                  ) : (
                    <span className="manifest-tag">Not sent</span>
                  )}
                </span>
              </li>
            </ul>

            <div className="feedback-summary-box">
              <p><strong>Reason:</strong> {selectedReason}</p>
              {notes.trim() && <p><strong>Notes:</strong> {notes}</p>}
            </div>

            {error && (
              <p className="dialog-error" role="alert">
                {error}
              </p>
            )}

            <div className="dialog-actions">
              <button
                className="secondary-button"
                type="button"
                onClick={() => setStep("form")}
                disabled={isSending}
              >
                Back
              </button>
              <button
                className="secondary-button"
                type="button"
                onClick={onClose}
                disabled={isSending}
              >
                Cancel
              </button>
              <button
                className="add-button"
                type="button"
                onClick={() => void handleSend()}
                disabled={isSending}
              >
                {isSending ? "Sending…" : "Send"}
              </button>
            </div>
          </div>
        )}
      </section>
    </div>
  );
}
