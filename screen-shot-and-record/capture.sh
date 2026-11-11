#!/bin/sh
# Act on a screenshot `screenshot-overlay` has already written.
#
# This is the QML plugin's `capture.sh` with the capturing half removed. There,
# the script ran grim/ImageMagick to produce the crop; here the Rust overlay has
# already produced it and told the plugin where it went, so what is left is the
# part that has to be a shell: redirect a file into a clipboard owner, run an
# interactive editor to completion, pipe tesseract's stdout somewhere.
#
#   capture.sh --action copy   --file <png>
#   capture.sh --action edit   --file <png> --output <png> [--editor <cmd>] [--keep-source]
#   capture.sh --action ocr    --file <png>
#   capture.sh --action search --file <png>
#
# It prints exactly one outcome token on stdout and **never notifies**:
#
#   copied  copy-failed  saved  edit-failed  ocr-copied  ocr-empty  ocr-failed
#   searched  search-failed  missing:<tool>  no-input  unknown-action
#
# The caller turns that into a translated `noctalia.notify`, which is where the
# message text, the on/off settings and the shell's own notification service all
# live — the QML version passed ten `--copied-title`-style arguments back the
# other way and then shelled out to `notify-send`. One token is also the only
# thing that survives the caller's transport: `noctalia.runStream` delivers
# stdout lines and **no exit status**, and it is chosen over `runAsync` because
# it has no timeout — an editor session and a slow OCR pass both outlive the
# 60 s ceiling `runAsync` puts on a child it waits for.
#
# `--keep-source` applies to `edit` only: it is the "keep the *_source.png the
# editor was given" setting. Every other action consumes its input.

set -u

ACTION=""
FILE=""
OUTPUT=""
EDITOR="swappy"
KEEP_SOURCE=0

while [ $# -gt 0 ]; do
    case "$1" in
        --action) ACTION="${2:-}"; shift 2 ;;
        --file) FILE="${2:-}"; shift 2 ;;
        --output) OUTPUT="${2:-}"; shift 2 ;;
        --editor) EDITOR="${2:-}"; shift 2 ;;
        --keep-source) KEEP_SOURCE=1; shift ;;
        *) shift ;;
    esac
done

# A dependency the caller's pre-flight missed — a bare `sh -lc` PATH is not
# necessarily the shell's. Reported as a token so it can be translated, rather
# than as a failure with no reason.
need() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "missing:$1"
        exit 0
    fi
}

if [ -z "$FILE" ] || [ ! -f "$FILE" ]; then
    echo "no-input"
    exit 0
fi

case "$ACTION" in
    copy)
        need wl-copy
        if wl-copy --type image/png <"$FILE"; then
            rm -f -- "$FILE"
            echo copied
        else
            echo copy-failed
        fi
        ;;

    edit)
        need "$EDITOR"
        # Interactive, and the user decides how long it stays open — which is why
        # this waits in the foreground of a timeout-less stream. The source is
        # removed only *after* the editor exits, or it would lose its input.
        case "${EDITOR##*/}" in
            satty) "$EDITOR" --filename "$FILE" --output-filename "$OUTPUT" ;;
            *) "$EDITOR" -f "$FILE" -o "$OUTPUT" ;;
        esac
        status=$?
        if [ "$KEEP_SOURCE" -eq 1 ]; then
            :
        else
            rm -f -- "$FILE"
        fi
        if [ "$status" -eq 0 ]; then
            echo saved
        else
            echo edit-failed
        fi
        ;;

    ocr)
        need tesseract
        need wl-copy
        # `$(...)` strips the trailing newline, so this is the same emptiness
        # test the QML script's `[[ -n "$text" ]]` made.
        TEXT=$(tesseract "$FILE" stdout 2>/dev/null) || TEXT=""
        rm -f -- "$FILE"
        if [ -z "$TEXT" ]; then
            echo ocr-empty
        elif printf '%s' "$TEXT" | wl-copy; then
            echo ocr-copied
        else
            echo ocr-failed
        fi
        ;;

    search)
        need curl
        need jq
        # Uploaded to whatever uguu.se is; Google Lens takes a URL, not a file.
        URL=$(curl -sf -F "files[]=@$FILE" https://uguu.se/upload | jq -r '.files[0].url // empty') || URL=""
        rm -f -- "$FILE"
        if [ -z "$URL" ]; then
            echo search-failed
        elif xdg-open "https://lens.google.com/uploadbyurl?url=$URL" >/dev/null 2>&1; then
            echo searched
        else
            echo search-failed
        fi
        ;;

    *)
        echo "unknown-action"
        ;;
esac
