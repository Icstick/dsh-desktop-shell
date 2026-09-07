// Environment list with single-active switching (M7-B, B1; env quick-edit D1/D2):
// every catalog environment as a card with ownership/profile/endpoint and
// Activate / Remove actions. Activation is a catalog-level switch; the
// ShellApp orchestrates stop-current → activate → start-target and
// stop-running → remove → state-reset flows around this component.

import { useState } from "react";

import { useI18n } from "../../../src/i18n";

import type { DshEnvironment, EnvironmentCatalog } from "../../../src/contracts";
import type { DesktopApi } from "../../../src/desktop-api";

interface EnvironmentListProps {
  api: DesktopApi;
  catalog: EnvironmentCatalog;
  activeEnvironmentId: string | null;
  /** True while the ShellApp orchestrates a switch or removal (disable all actions). */
  transitioning: boolean;
  /** Id of the managed environment with a running DSH (drives the remove notice). */
  runningEnvironmentId: string | null;
  onActivated(catalog: EnvironmentCatalog, environment: DshEnvironment): void;
  /** Open the create-mode wizard (D1: trigger-based, never pre-filled). */
  onAddEnvironment(): void;
  /** Open the sectioned edit form for one environment (D3). */
  onEdit(environment: DshEnvironment): void;
  /**
   * Remove one environment; the ShellApp orchestrates stop → remove and
   * resets the active state when the removed one was active. Rejections
   * surface here as the list-level error.
   */
  onRemove(environment: DshEnvironment): Promise<void>;
}

export function EnvironmentList({
  api,
  catalog,
  activeEnvironmentId,
  transitioning,
  runningEnvironmentId,
  onActivated,
  onAddEnvironment,
  onEdit,
  onRemove,
}: EnvironmentListProps) {
  const { t } = useI18n();
  const [error, setError] = useState<string | null>(null);
  const [activatingId, setActivatingId] = useState<string | null>(null);
  const [confirmingRemoveId, setConfirmingRemoveId] = useState<string | null>(null);
  const [removing, setRemoving] = useState(false);

  const activate = async (environment: DshEnvironment) => {
    setError(null);
    setActivatingId(environment.id);
    try {
      const next = await api.setActiveEnvironment({
        schemaVersion: 1,
        environmentId: environment.id,
      });
      onActivated(next, environment);
    } catch {
      setError(t("envlist.errorActivate"));
    } finally {
      setActivatingId(null);
    }
  };

  const requestRemove = async (environment: DshEnvironment) => {
    setError(null);
    setRemoving(true);
    try {
      await onRemove(environment);
      setConfirmingRemoveId(null);
    } catch (error: unknown) {
      // Surface the backend message when present (DesktopCommandError), so
      // catalog/validation failures are debuggable instead of generic copy.
      const message =
        typeof error === "object" &&
        error !== null &&
        "message" in error &&
        typeof (error as { message?: unknown }).message === "string"
          ? (error as { message: string }).message
          : null;
      setError(message || t("envlist.errorRemove"));
    } finally {
      setRemoving(false);
    }
  };

  const addButton = (
    <button
      type="button"
      className="environment-list__add"
      onClick={onAddEnvironment}
      disabled={transitioning}
      data-testid="add-environment"
    >
      {t("envlist.addEnvironment")}
    </button>
  );

  if (catalog.environments.length === 0) {
    return (
      <section className="environment-list" data-testid="environment-list">
        <h2>{t("envlist.title")}</h2>
        <p className="environment-list__empty">{t("envlist.empty")}</p>
        {addButton}
      </section>
    );
  }

  return (
    <section className="environment-list" data-testid="environment-list">
      <div className="environment-list__header">
        <h2>{t("envlist.title")}</h2>
        {addButton}
      </div>
      {error && (
        <p className="environment-list__error" role="alert">
          {error}
        </p>
      )}
      <ul className="environment-list__items">
        {catalog.environments.map((environment) => {
          const active = environment.id === activeEnvironmentId;
          const running = environment.id === runningEnvironmentId;
          const confirming = confirmingRemoveId === environment.id;
          const disabled = transitioning || removing || activatingId !== null;
          return (
            <li
              key={environment.id}
              className={"environment-list__item" + (active ? " is-active" : "")}
              data-testid={"environment-" + environment.id}
            >
              <div className="environment-list__info">
                <strong>{environment.label}</strong>
                <code>{environment.id}</code>
                <span className="environment-list__meta">
                  {environment.ownership}
                  {environment.profile ? " · " + environment.profile : ""}
                  {" · " + environment.endpoint.port}
                </span>
              </div>
              <div className="environment-list__actions">
                {active ? (
                  <span className="environment-list__badge">active</span>
                ) : (
                  <button
                    type="button"
                    className="environment-list__activate"
                    onClick={() => void activate(environment)}
                    disabled={disabled}
                    data-testid={"activate-" + environment.id}
                  >
                    {activatingId === environment.id
                      ? t("envlist.switching")
                      : t("envlist.activate")}
                  </button>
                )}
                {!confirming && (
                  <button
                    type="button"
                    className="environment-list__edit"
                    onClick={() => {
                      setError(null);
                      onEdit(environment);
                    }}
                    disabled={disabled}
                    data-testid={"edit-" + environment.id}
                  >
                    {t("envlist.edit")}
                  </button>
                )}
                {!confirming && (
                  <button
                    type="button"
                    className="environment-list__remove"
                    onClick={() => {
                      setError(null);
                      setConfirmingRemoveId(environment.id);
                    }}
                    disabled={disabled}
                    data-testid={"remove-" + environment.id}
                  >
                    {t("envlist.remove")}
                  </button>
                )}
              </div>
              {confirming && (
                <div
                  className="environment-list__confirm"
                  role="alertdialog"
                  aria-label={environment.label}
                >
                  <p className="environment-list__confirm-body">
                    {t("envlist.removeBody", { label: environment.label })}
                  </p>
                  {active && (
                    <p
                      className="environment-list__confirm-note"
                      data-testid={"remove-note-active-" + environment.id}
                    >
                      {t("envlist.removeActiveNote")}
                    </p>
                  )}
                  {running && (
                    <p
                      className="environment-list__confirm-note"
                      data-testid={"remove-note-running-" + environment.id}
                    >
                      {t("envlist.removeRunningNote")}
                    </p>
                  )}
                  <div className="environment-list__confirm-actions">
                    <button
                      type="button"
                      className="environment-list__confirm-cancel"
                      onClick={() => setConfirmingRemoveId(null)}
                      disabled={removing}
                      data-testid={"remove-cancel-" + environment.id}
                    >
                      {t("common.cancel")}
                    </button>
                    <button
                      type="button"
                      className="environment-list__confirm-ok"
                      onClick={() => void requestRemove(environment)}
                      disabled={removing}
                      data-testid={"remove-confirm-" + environment.id}
                    >
                      {removing ? t("envlist.switching") : t("envlist.removeConfirm")}
                    </button>
                  </div>
                </div>
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}
