import { useEffect, useState } from "react";
import { Grid, MixKachel, PageHeader, ZurueckKnopf } from "../components/Cards";
import { EmptyState } from "../components/EmptyState";
import { SparkIcon } from "../components/Icons";
import { api, fallback } from "../lib/api";
import { t } from "../lib/i18n";
import { useLibrary } from "../store/library";
import type { WeeklyMixSummary } from "../types";

// every weekly mix, as far back as the listening history reaches.
//
// three of them stand on the home page, and the rest used to be reachable
// only through a horizontal row to swipe, of which two and a half tiles were
// visible on a phone
export function MixesPage() {
  const revision = useLibrary((s) => s.revision);
  // `null` means not looked yet. only afterwards may the page claim there
  // are no mixes
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
