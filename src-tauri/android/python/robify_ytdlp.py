"""yt-dlp on android, run inside the app's own python.

on the desktop yt-dlp is a program that robify starts and whose output it
reads line by line. there is no such program on android: python lives in the
app as a library, and yt-dlp is a package inside it. so it is called here in
the same process, and this module makes that look like a process from the
outside — an exit code, standard output, standard error.

the three things the rust side expects are kept:

* `ROBIFYPROGRESS` lines are read as they are written, not at the end, so the
  progress bar keeps moving during a download
* a job can be cancelled while it runs
* the answer is the same json object the bridge always returned
"""

import io
import json
import sys
import threading
import traceback

# what a running job knows about itself, by job id
_LAEUFT = {}
_SPERRE = threading.Lock()

# yt-dlp writes this in front of every progress line, see
# `PROGRESS_MARKER` in downloader.rs
_MARKE = "ROBIFYPROGRESS"


def _zahl(text):
    """a number out of a progress field, or None where yt-dlp wrote "NA"."""
    try:
        return float(text.strip())
    except (TypeError, ValueError):
        return None


class _Abgebrochen(Exception):
    """raised inside yt-dlp's own output, which stops it wherever it stands."""


class _Mitleser(io.TextIOBase):
    """collects what yt-dlp writes and watches it go by.

    reading the progress only at the end would leave the bar at zero for the
    whole download and jump to a hundred. so every line is looked at as it
    arrives, and that is also the moment to notice a cancellation: yt-dlp
    writes often enough for it to take effect within fractions of a second.
    """

    def __init__(self, kennung, sammlung):
        self._kennung = kennung
        self._sammlung = sammlung
        self._rest = ""

    def write(self, text):
        if not text:
            return 0

        # split at carriage return as well, not at the line break alone:
        # yt-dlp writes its progress over the same line again and again and
        # ends it with `\r`. reading only at `\n` the whole run arrives as one
        # line at the end, the progress bar never moves — and on android the
        # download is cut off after two minutes without any sign of life
        self._rest += text
        while True:
            stelle = min(
                (self._rest.find(z) for z in ("\r", "\n") if z in self._rest),
                default=-1,
            )
            if stelle < 0:
                break
            zeile, self._rest = self._rest[:stelle], self._rest[stelle + 1:]
            if not self._fortschritt_lesen(zeile):
                # progress lines are not kept: they have done their work, and
                # in an error message a few hundred of them bury the sentence
                # that says what went wrong
                self._sammlung.append(zeile + "\n")

        # checked after writing, so the last line still reaches the log
        stand = _LAEUFT.get(self._kennung)
        if stand is not None and stand.get("abbruch"):
            raise _Abgebrochen()
        return len(text)

    def _fortschritt_lesen(self, zeile):
        """reads a progress line, and says whether it was one."""
        stelle = zeile.find(_MARKE)
        if stelle < 0:
            return False
        # the shape comes from `--progress-template` in downloader.rs:
        # ROBIFYPROGRESS|geladen|gesamt|schaetzung|tempo|rest
        felder = zeile[stelle + len(_MARKE):].strip().split("|")
        if len(felder) < 4:
            return True
        geladen = _zahl(felder[1])
        gesamt = _zahl(felder[2])
        if gesamt is None:
            # a stream names no total, only an estimate
            gesamt = _zahl(felder[3])
        if geladen is None or not gesamt or gesamt <= 0:
            return True
        prozent = max(0.0, min(100.0, geladen / gesamt * 100.0))
        stand = _LAEUFT.get(self._kennung)
        if stand is not None:
            stand["prozent"] = prozent
        return True

    def rest_holen(self):
        """what stands after the last line break.

        yt-dlp does not end every output with one — `--print` writes the last
        entry without it, and the search would lose exactly that line
        """
        rest, self._rest = self._rest, ""
        if rest and not self._fortschritt_lesen(rest):
            self._sammlung.append(rest)

    def flush(self):
        return None

    def writable(self):
        return True


def lauf(kennung, args):
    """runs yt-dlp with the given arguments and answers as the bridge does."""
    import yt_dlp

    ausgabe = []
    with _SPERRE:
        _LAEUFT[kennung] = {"prozent": 0.0, "abbruch": False}

    mitleser = _Mitleser(kennung, ausgabe)
    vorher_out, vorher_err = sys.stdout, sys.stderr
    code = 0
    fehlertext = ""

    try:
        sys.stdout = mitleser
        sys.stderr = mitleser
        try:
            yt_dlp.main(list(args))
        except SystemExit as ende:
            code = ende.code if isinstance(ende.code, int) else 1
    except _Abgebrochen:
        code = -1
        fehlertext = "Abgebrochen."
    except Exception:  # noqa: BLE001 — the caller wants the text, not a type
        code = -1
        # the whole traceback: on the phone there is no second chance to look,
        # and a bare type name says nothing about where it came from
        fehlertext = traceback.format_exc()
    finally:
        # whatever stands after the last line break belongs to the output too
        try:
            mitleser.rest_holen()
        except Exception:  # noqa: BLE001 — nothing here may swallow the result
            pass
        sys.stdout, sys.stderr = vorher_out, vorher_err
        with _SPERRE:
            _LAEUFT.pop(kennung, None)

    # both streams arrive as one: yt-dlp mixes progress and diagnosis, and in
    # one process they cannot be told apart afterwards.
    #
    # what goes into `err` decides how the failure reads for the user: the
    # rust side looks in there for the reason — a 403, a missing format, a
    # refusal. where nothing went wrong it stays empty, otherwise a successful
    # download would look like one with an error message
    text = "".join(ausgabe)
    if fehlertext:
        fehler = f"{text}\n{fehlertext}" if text else fehlertext
    elif code != 0:
        fehler = text
    else:
        fehler = ""

    return json.dumps(
        {"code": code, "out": text, "err": fehler},
        ensure_ascii=False,
    )


def fortschritt(kennung):
    """percent of a running job, -1 where none is running under that name."""
    stand = _LAEUFT.get(kennung)
    return float(stand["prozent"]) if stand else -1.0


def abbrechen(kennung):
    """marks a job for cancellation; it stops at its next output."""
    stand = _LAEUFT.get(kennung)
    if stand is not None:
        stand["abbruch"] = True


def fassung():
    """the version of yt-dlp built into the app."""
    try:
        import yt_dlp

        return yt_dlp.version.__version__
    except Exception as fehler:  # noqa: BLE001
        return f"unbekannt ({fehler!r})"
