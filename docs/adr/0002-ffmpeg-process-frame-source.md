# FFmpeg Process Frame Source

The initial Frame Source invokes `ffmpeg` as a subprocess instead of binding to
native FFmpeg or OpenCV libraries. This keeps local and CI setup lighter for
coding agents while preserving a narrow adapter boundary that can later be
replaced by a native decoder.
