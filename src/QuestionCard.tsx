import { FormEvent, useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";

export type QuestionData = {
  isQuestion?: boolean;
  questionId?: string;
  question?: string;
  options?: string[];
  /** Set once the user answers, so a re-rendered card can show the choice. */
  answer?: string;
  /** False for unattended runs (Max mode), where the run did not wait. */
  awaitingAnswer?: boolean;
};

type QuestionCardProps = {
  data: QuestionData;
  /** The answer for this question, if one has been given. */
  answered?: string;
  /** The agent is still parked on this question and can accept an answer. */
  canAnswer: boolean;
  /** An agent run is in progress (used only to explain an unavailable question). */
  runActive?: boolean;
  onAnswer: (questionId: string, answer: string) => void;
};

/**
 * Renders an `ask_user` question inside the chat stream: the prompt, an
 * optional row of choice buttons, and a free-form field that is always
 * available. Answering behaves like a normal send, but it resumes the agent's
 * parked tool call instead of starting a new turn.
 */
export default function QuestionCard({ data, answered, canAnswer, runActive = false, onAnswer }: QuestionCardProps) {
  const [freeform, setFreeform] = useState("");
  const questionId = data.questionId ?? "";
  const options = data.options ?? [];
  const answer = answered ?? data.answer;
  const isAnswered = Boolean(answer);
  const question = data.question ?? "The agent has a question.";

  function submit(value: string) {
    const trimmed = value.trim();
    if (!trimmed || !questionId || !canAnswer) return;
    onAnswer(questionId, trimmed);
    setFreeform("");
  }

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    submit(freeform);
  }

  return (
    <article className={`question-card ${isAnswered ? "answered" : canAnswer ? "waiting" : "stale"}`}>
      <header className="question-card-header">
        <span className="question-badge">
          <span className="question-icon" aria-hidden="true">❓</span> Question
        </span>
        {isAnswered ? (
          <span className="question-status answered">✓ Answered</span>
        ) : canAnswer ? (
          <span className="question-status waiting">
            <span className="question-pulse" aria-hidden="true" /> Waiting for you
          </span>
        ) : (
          <span className="question-status stale">No longer waiting</span>
        )}
      </header>

      <div className="question-body">
        <div className="message-content">
          <Markdown remarkPlugins={[remarkGfm]}>{question}</Markdown>
        </div>
      </div>

      {isAnswered ? (
        <div className="question-answer">
          <span className="question-answer-label">Your answer</span>
          <p>{answer}</p>
        </div>
      ) : canAnswer ? (
        <div className="question-controls">
          {options.length > 0 && (
            <div className="question-options">
              {options.map((option) => (
                <button
                  className="question-option-btn"
                  type="button"
                  key={option}
                  onClick={() => submit(option)}
                >
                  {option}
                </button>
              ))}
            </div>
          )}
          <form className="question-freeform" onSubmit={handleSubmit}>
            <input
              className="question-input"
              type="text"
              value={freeform}
              placeholder={options.length > 0 ? "Or type your own answer…" : "Type your answer…"}
              onChange={(event) => setFreeform(event.target.value)}
            />
            <button
              className="question-submit-btn"
              type="submit"
              disabled={!freeform.trim()}
            >
              Send
            </button>
          </form>
        </div>
      ) : runActive ? (
        <div className="question-controls">
          <p className="question-note">
            The agent continued without waiting for this answer.
          </p>
        </div>
      ) : null}
    </article>
  );
}
