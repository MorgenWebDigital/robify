import { useEffect, useState } from "react";
import { t } from "../lib/i18n";
import { CheckIcon, CloseIcon, PlusIcon } from "./Icons";
import { Button, Field, inputClass, Modal } from "./Modal";
import { applyAccent, useLibrary } from "../store/library";
import { akzentSchrift, eigeneFarben, normalisiereHex } from "../lib/farbe";

/**
 * Vorgabefarben. Die erste ist die Werkseinstellung.
 *
 * Ausgesucht aus den CSS-Standardfarben: ein neutrales Grau, dann je ein
 * kräftiger, dunkler Ton pro Richtung. Die Schrift darauf wird gerechnet,
 * nicht festgelegt, vier der sechs sind zu dunkel für dunklen Text.
 */
const VORGABEN = [
  "#a8a8b3", // Grau
  "#daa520", // Goldenrod
  "#8b0000", // DarkRed
  "#4b0082", // Indigo
  "#191970", // MidnightBlue
  "#006400", // DarkGreen
];

export function AkzentWahl() {
  const settings = useLibrary((s) => s.settings);
  const saveSetting = useLibrary((s) => s.saveSetting);
  const [offen, setOffen] = useState(false);

  if (!settings) return null;

  const eigene = eigeneFarben(settings.accentCustom);
  // Eine selbst gemischte Farbe, die schon als Vorgabe existiert, wäre ein
  // zweites gleiches Feld in derselben Reihe.
  const alle = [
    ...VORGABEN,
    ...eigene.filter((farbe) => !VORGABEN.includes(farbe)),
  ];

  /**
   * Entfernt eine selbst gemischte Farbe.
   *
   * Ist sie gerade in Gebrauch, fällt die App auf die Werkseinstellung
   * zurück, sonst bliebe eine Farbe aktiv, die in keiner Reihe mehr steht
   * und sich nicht wiederfinden ließe.
   */
  const loeschen = async (farbe: string) => {
    await saveSetting(
      "accentCustom",
      eigene.filter((f) => f !== farbe).join(","),
    );
    if (settings.accent.toLowerCase() === farbe)
      await saveSetting("accent", VORGABEN[0]);
  };

  return (
    <>
      <div className="flex flex-wrap items-center gap-3">
        {alle.map((farbe) => {
          const gewaehlt = settings.accent.toLowerCase() === farbe;
          const eigen = !VORGABEN.includes(farbe);
          return (
            // Der Löschknopf sitzt am Feld und braucht darum einen eigenen
            // Bezugsrahmen; ohne ihn säße er am Rand der ganzen Reihe.
            <span key={farbe} className="group relative inline-flex">
              <button
                type="button"
                aria-label={t("Akzentfarbe {0}", farbe)}
                title={farbe}
                aria-pressed={gewaehlt}
                onClick={() => void saveSetting("accent", farbe)}
                className="swatch h-9 w-9"
                style={
                  {
                    "--feld": farbe,
                    color: akzentSchrift(farbe),
                  } as React.CSSProperties
                }
              >
                <CheckIcon size={16} />
              </button>

              {eigen && (
                // Erst beim Zeigen sichtbar, damit die Reihe ruhig bleibt,
                // aber mit der Tastatur jederzeit erreichbar.
                <button
                  type="button"
                  onClick={() => void loeschen(farbe)}
                  aria-label={t("Eigene Farbe {0} löschen", farbe)}
                  title={t("Farbe löschen")}
                  className="pill-btn is-raised absolute -top-1.5 -end-1.5 h-4 w-4 opacity-0 transition group-hover:opacity-100 focus-visible:opacity-100"
                >
                  <CloseIcon size={9} />
                </button>
              )}
            </span>
          );
        })}

        <button
          type="button"
          onClick={() => setOffen(true)}
          aria-label={t("Eigene Farbe anlegen")}
          title={t("Eigene Farbe anlegen")}
          className="pill-btn is-raised h-9 w-9"
        >
          <PlusIcon size={16} />
        </button>
      </div>

      <FarbDialog open={offen} onClose={() => setOffen(false)} />
    </>
  );
}

function FarbDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const settings = useLibrary((s) => s.settings);
  const saveSetting = useLibrary((s) => s.saveSetting);

  const [entwurf, setEntwurf] = useState("#a8a8b3");
  const [text, setText] = useState("#a8a8b3");

  // Beim Öffnen an der aktuellen Farbe anknüpfen: Meist will man sie
  // nachjustieren, nicht bei Grau von vorn anfangen.
  useEffect(() => {
    if (!open || !settings) return;
    const start = normalisiereHex(settings.accent) ?? "#a8a8b3";
    setEntwurf(start);
    setText(start);
  }, [open, settings?.accent]);

  /**
   * Der Entwurf färbt die App sofort ein, ohne ihn zu speichern.
   *
   * Eine Akzentfarbe beurteilt man nicht an einem Farbfeld, sondern an den
   * Knöpfen und Balken, auf denen sie später liegt. Beim Schließen ohne
   * Speichern wird die gespeicherte Farbe wiederhergestellt.
   */
  useEffect(() => {
    if (!open) return;
    applyAccent(entwurf);
    return () => {
      if (settings) applyAccent(settings.accent);
    };
  }, [open, entwurf, settings?.accent]);

  if (!settings) return null;

  const eigene = eigeneFarben(settings.accentCustom);
  const gueltig = normalisiereHex(text);

  const uebernehmen = async () => {
    const farbe = normalisiereHex(text);
    if (!farbe) return;
    // Doppelte Einträge und solche, die es schon als Vorgabe gibt, bringen
    // nichts, die Reihe soll übersichtlich bleiben.
    if (!VORGABEN.includes(farbe) && !eigene.includes(farbe)) {
      await saveSetting("accentCustom", [...eigene, farbe].join(","));
    }
    await saveSetting("accent", farbe);
    onClose();
  };

  const entfernen = async (farbe: string) => {
    await saveSetting(
      "accentCustom",
      eigene.filter((f) => f !== farbe).join(","),
    );
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t("Eigene Akzentfarbe")}
      width="max-w-md"
      footer={
        <>
          <Button onClick={onClose} variant="ghost">
            {t("Abbrechen")}
          </Button>
          <Button
            onClick={() => void uebernehmen()}
            variant="outline"
            disabled={!gueltig}
          >
            {t("Speichern")}
          </Button>
        </>
      }
    >
      <div className="space-y-5">
        <div className="flex items-center gap-4">
          <div
            className="swatch h-14 w-14 shrink-0"
            style={{ "--feld": entwurf } as React.CSSProperties}
            aria-hidden="true"
          />
          <div className="min-w-0 flex-1 space-y-3">
            <Field label={t("Farbwert")}>
              <input
                value={text}
                onChange={(event) => {
                  setText(event.target.value);
                  const farbe = normalisiereHex(event.target.value);
                  if (farbe) setEntwurf(farbe);
                }}
                placeholder="#a8a8b3"
                spellCheck={false}
                className={`${inputClass} font-mono ${gueltig ? "" : "border-danger"}`}
              />
            </Field>
            <input
              type="color"
              value={entwurf}
              onChange={(event) => {
                setEntwurf(event.target.value);
                setText(event.target.value);
              }}
              aria-label={t("Farbe auswählen")}
              className="h-9 w-full cursor-pointer rounded-lg border border-ink-600 bg-transparent"
            />
          </div>
        </div>

        <p className="text-xs text-mute">
          {gueltig
            ? t(
                "Die App zeigt die Farbe schon jetzt. Gespeichert wird sie erst beim Übernehmen.",
              )
            : t("Sechs Stellen wie #a8a8b3, die Kurzform #abc geht auch.")}
        </p>

        {eigene.length > 0 && (
          <div>
            <span className="eyebrow mb-2 block">
              {t("Gespeicherte Farben")}
            </span>
            <ul className="flex flex-wrap gap-2">
              {eigene.map((farbe) => (
                <li
                  key={farbe}
                  className="flex items-center gap-1.5 rounded-full pe-1 ps-1"
                >
                  <button
                    type="button"
                    onClick={() => {
                      setEntwurf(farbe);
                      setText(farbe);
                    }}
                    title={`${farbe} bearbeiten`}
                    className="swatch h-7 w-7"
                    style={{ "--feld": farbe } as React.CSSProperties}
                  />
                  <button
                    type="button"
                    onClick={() => void entfernen(farbe)}
                    aria-label={t("{0} entfernen", farbe)}
                    title={t("Entfernen")}
                    className="pill-btn h-6 w-6"
                  >
                    <CloseIcon size={12} />
                  </button>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </Modal>
  );
}
