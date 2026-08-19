import { useEffect, useState } from "react";
import { Grid, MixKachel, PageHeader, ZurueckKnopf } from "../components/Cards";
import { EmptyState } from "../components/EmptyState";
import { SparkIcon } from "../components/Icons";
import { api, fallback } from "../lib/api";
import { t } from "../lib/i18n";
import { useLibrary } from "../store/library";
import type { WeeklyMixSummary } from "../types";

/**
 * Alle Wochenmixe, so weit die Hörhistorie reicht.
 *
 * Auf der Startseite stehen drei davon; die übrigen waren vorher nur über eine
 * waagerechte Reihe zum Schieben erreichbar, von der auf einem Telefon
 * zweieinhalb Kacheln zu sehen waren.
 */
export function MixesPage() {
  const revision = useLibrary((s) => s.revision);
  // `null` heißt: noch nicht nachgesehen. Erst danach darf die Seite
  // behaupten, es gebe keine Mixe.
  const [mixes, setMixes] = useState<WeeklyMixSummary[] | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .weeklyMixes(52)
      .then((value) => {
        if (!cancelled) setMixes(value);
      })
      .catch((error) => {
        if (!cancelled) setMixes(fallback([], t("Wochenmixe"))(error));
      });
    return () => {
      cancelled = true;
    };
  }, [revision]);

  return (
    <div>
      <ZurueckKnopf ziel="/" />
      <PageHeader
        eyebrow={t("Sammlung")}
        title={t("Wochenmixe")}
        subtitle={t("Die 30 meistgehörten Titel je Woche")}
      />

      {mixes === null ? null : mixes.length === 0 ? (
        <EmptyState
          icon={SparkIcon}
          title={t("Noch keine Wochenmixe")}
          text={t("Der Wochenmix entsteht, sobald du etwas gehört hast.")}
        />
      ) : (
        <Grid>
          {mixes.map((mix) => (
            <MixKachel key={mix.weekKey} mix={mix} />
          ))}
        </Grid>
      )}
    </div>
  );
}
