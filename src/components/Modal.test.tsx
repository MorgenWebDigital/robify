import { useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Field, inputClass, Modal } from "./Modal";

// a dialog built like the real ones: the close callback comes into being
// anew at every render, and an invisible file input stands before the name
// field.
//
// both together were the bug one saw when creating a playlist: one letter,
// then the focus jumped away. the replica has to carry these two quirks,
// otherwise the test checks something other than what the app does
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
  // the actual bug: the focus effect hung off `onClose`, and that is a
  // different function at every render. every keystroke made it run again and
  // set the focus anew
  it("behält den Fokus im Feld, während man tippt", async () => {
    const nutzer = userEvent.setup();
    render(<Beispiel />);

    const feld = screen.getByLabelText("Name");
    await nutzer.click(feld);
    await nutzer.keyboard("Abend");

    expect(feld).toHaveValue("Abend");
    expect(feld).toHaveFocus();
  });

  // the second part of the same bug: the aim was the first control that is
  // not the close cross, and that was the hidden file input of the cover
  // field
  it("setzt den Fokus nicht auf ein verborgenes Feld", () => {
    render(<Beispiel />);
    expect(document.activeElement).not.toHaveAttribute("type", "file");
    expect(screen.getByLabelText("Name")).toHaveFocus();
  });

  it("schließt mit Escape, auch nach einer Änderung im Dialog", async () => {
    const nutzer = userEvent.setup();
    const geschlossen = vi.fn();
    render(<Beispiel onClose={geschlossen} />);

    // type first: afterwards a different callback stands in the dialog than
    // at build-up. an old callback held on to would show up here
    await nutzer.keyboard("x");
    await nutzer.keyboard("{Escape}");

    expect(geschlossen).toHaveBeenCalledTimes(1);
  });

  // the dialog is drawn onto the body and not where it stands in the tree.
  // otherwise `fixed` aligns to the page area because that one carries a
  // `transform`, and the dialogs stand centred over the content instead of
  // over the window
  it("zeichnet sich unmittelbar an den Körper", () => {
    const { container } = render(<Beispiel />);
    expect(container).toBeEmptyDOMElement();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });
});
