import { useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Field, inputClass, Modal } from "./Modal";

/**
 * Ein Dialog, gebaut wie die echten: Der Rückruf zum Schließen entsteht bei
 * jedem Zeichnen neu, und vor dem Namensfeld steht eine unsichtbare
 * Dateiauswahl.
 *
 * Beides zusammen war der Fehler, den man beim Anlegen einer Playlist sah:
 * ein Buchstabe, dann sprang der Fokus fort. Der Nachbau muss diese zwei
 * Eigenheiten haben, sonst prüft der Test etwas anderes als die App tut.
 */
function Beispiel({ onClose = () => {} }: { onClose?: () => void }) {
  const [name, setName] = useState("");
  return (
    <Modal
      open
      title="Neue Playlist"
      onClose={() => {
        setName("");
        onClose();
      }}
    >
      <Field label="Cover">
        <input type="file" className="hidden" />
      </Field>
      <Field label="Name">
        <input
          autoFocus
          aria-label="Name"
          value={name}
          onChange={(event) => setName(event.target.value)}
          className={inputClass}
        />
      </Field>
    </Modal>
  );
}

describe("Modal", () => {
  /**
   * Der eigentliche Fehler: Der Fokus-Effekt hing an `onClose`, und das ist
   * bei jedem Zeichnen eine andere Funktion. Jeder Tastendruck ließ ihn erneut
   * laufen und den Fokus neu setzen.
   */
  it("behält den Fokus im Feld, während man tippt", async () => {
    const nutzer = userEvent.setup();
    render(<Beispiel />);

    const feld = screen.getByLabelText("Name");
    await nutzer.click(feld);
    await nutzer.keyboard("Abend");

    expect(feld).toHaveValue("Abend");
    expect(feld).toHaveFocus();
  });

  /**
   * Der zweite Teil desselben Fehlers: Gezielt wurde auf „das erste
   * Bedienelement, das kein Schließkreuz ist“, und das war die verborgene
   * Dateiauswahl des Cover-Feldes.
   */
  it("setzt den Fokus nicht auf ein verborgenes Feld", () => {
    render(<Beispiel />);
    expect(document.activeElement).not.toHaveAttribute("type", "file");
    expect(screen.getByLabelText("Name")).toHaveFocus();
  });

  it("schließt mit Escape, auch nach einer Änderung im Dialog", async () => {
    const nutzer = userEvent.setup();
    const geschlossen = vi.fn();
    render(<Beispiel onClose={geschlossen} />);

    // Erst tippen: Danach steht im Dialog ein anderer Rückruf als beim
    // Aufbau. Ein festgehaltener alter Rückruf fiele hier auf.
    await nutzer.keyboard("x");
    await nutzer.keyboard("{Escape}");

    expect(geschlossen).toHaveBeenCalledTimes(1);
  });

  /**
   * Der Dialog wird an den Körper gezeichnet, nicht dorthin, wo er im Baum
   * steht. Sonst richtet `fixed` sich am Seitenbereich aus, weil der eine
   * `transform` trägt, und die Dialoge stehen mittig über dem Inhalt statt
   * über dem Fenster.
   */
  it("zeichnet sich unmittelbar an den Körper", () => {
    const { container } = render(<Beispiel />);
    expect(container).toBeEmptyDOMElement();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });
});
