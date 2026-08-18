import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Menu } from "./Menu";

const EINTRAEGE = [
  { label: "Als Nächstes spielen", onSelect: vi.fn() },
  { label: "Zur Warteschlange", onSelect: vi.fn() },
  { label: "Löschen", onSelect: vi.fn() },
];

/**
 * Legt fest, wo der Knopf im Fenster sitzt.
 *
 * jsdom rechnet kein Layout und liefert für jedes Element ein Rechteck aus
 * lauter Nullen. Die Richtungswahl des Menüs hängt aber genau daran, also
 * muss der Test sie vorgeben.
 */
function knopfSitztBei(oben: number) {
  window.innerHeight = 800;
  window.innerWidth = 1200;
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({
    top: oben,
    bottom: oben + 32,
    left: 100,
    right: 132,
    width: 32,
    height: 32,
    x: 100,
    y: oben,
    toJSON: () => ({}),
  });
}

afterEach(() => vi.restoreAllMocks());

describe("Menu", () => {
  /**
   * Die Liste hängt am Fenster und wird an den Körper gezeichnet. Stünde sie
   * im Baum neben dem Knopf, zählte sie zur Bildlauffläche ihres Behälters:
   * Beim letzten Titel einer Playlist wuchs die Seite um ihre Höhe.
   */
  it("zeichnet die Liste an den Körper, nicht neben den Knopf", async () => {
    const nutzer = userEvent.setup();
    knopfSitztBei(100);
    const { container } = render(
      <Menu
        items={EINTRAEGE}
        trigger={({ toggle }) => (
          <button type="button" onClick={toggle}>
            Mehr
          </button>
        )}
      />,
    );

    await nutzer.click(screen.getByRole("button", { name: "Mehr" }));

    const liste = screen.getByRole("menu");
    expect(liste).toBeInTheDocument();
    expect(container.contains(liste)).toBe(false);
    expect(liste).toHaveStyle({ position: "fixed" });
  });

  it("fährt nach unten aus, wenn darunter Platz ist", async () => {
    const nutzer = userEvent.setup();
    knopfSitztBei(100);
    render(
      <Menu
        items={EINTRAEGE}
        trigger={({ toggle }) => (
          <button type="button" onClick={toggle}>
            Mehr
          </button>
        )}
      />,
    );

    await nutzer.click(screen.getByRole("button", { name: "Mehr" }));

    const liste = screen.getByRole("menu");
    expect(liste.style.top).not.toBe("");
    expect(liste.style.bottom).toBe("");
  });

  /** Am unteren Rand kehrt sich die Richtung um, statt aus dem Bild zu laufen. */
  it("fährt nach oben aus, wenn darunter kein Platz mehr ist", async () => {
    const nutzer = userEvent.setup();
    knopfSitztBei(760);
    render(
      <Menu
        items={EINTRAEGE}
        trigger={({ toggle }) => (
          <button type="button" onClick={toggle}>
            Mehr
          </button>
        )}
      />,
    );

    await nutzer.click(screen.getByRole("button", { name: "Mehr" }));

    const liste = screen.getByRole("menu");
    expect(liste.style.bottom).not.toBe("");
    expect(liste.style.top).toBe("");
  });
});
