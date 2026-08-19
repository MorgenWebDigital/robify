import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Menu } from "./Menu";

const EINTRAEGE = [
  { label: "Als Nächstes spielen", onSelect: vi.fn() },
  { label: "Zur Warteschlange", onSelect: vi.fn() },
  { label: "Löschen", onSelect: vi.fn() },
];

// pins down where the button sits in the window.
//
// jsdom computes no layout and returns a rectangle of nothing but zeros for
// every element. the direction the menu picks hangs on exactly that, so the
// test has to supply it
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
  // the list hangs off the window and is drawn onto the body. standing in
  // the tree next to the button, it would count towards the scrolling area of
  // its container: at the last track of a playlist the page grew by its
  // height
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

  /** at the bottom edge the direction turns around instead of running out of view. */
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
