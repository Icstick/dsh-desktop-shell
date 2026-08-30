// Environment list with single-active switching (M7-B, B1): every catalog
// environment as a card with its ownership/profile and an Activate action.
// Activation is a catalog-level switch; the ShellApp orchestrates the
// stop-current → activate → start-target flow around this component.

import { useState } from "react";

import type { DshEnvironment, EnvironmentCatalog } from "../../../src/contracts";
import type { DesktopApi } from "../../../src/desktop-api";

interface EnvironmentListProps {
  api: DesktopApi;
  catalog: EnvironmentCatalog;
  activeEnvironmentId: string | null;
  /** True while the ShellApp orchestrates a switch (disable all actions). */
  transitioning: boolean;
  onActivated(catalog: EnvironmentCatalog, environment: DshEnvironment): void;
}

export function EnvironmentList({
  api,
  catalog,
  activeEnvironmentId,
  transitioning,
  onActivated,
}: EnvironmentListProps) {
  const [error, setError] = useState<string | null>(null);
  const [activatingId, setActivatingId] = useState<string | null>(null);

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
      setError("The environment could not be activated.");
    } finally {
      setActivatingId(null);
    }
  };

  if (catalog.environments.length === 0) {
    return (
      <section className="environment-list" data-testid="environment-list">
        <h2>Environments</h2>
        <p className="environment-list__empty">No environments saved yet — use the wizard above.</p>
      </section>
    );
  }

  return (
    <section className="environment-list" data-testid="environment-list">
      <h2>Environments</h2>
      {error && (
        <p className="environment-list__error" role="alert">
          {error}
        </p>
      )}
      <ul className="environment-list__items">
        {catalog.environments.map((environment) => {
          const active = environment.id === activeEnvironmentId;
          const disabled = transitioning || activatingId !== null;
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
                </span>
              </div>
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
                  {activatingId === environment.id ? "Switching…" : "Activate"}
                </button>
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}
