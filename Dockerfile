# syntax=docker/dockerfile:1
FROM docker.io/library/rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src src
COPY web web
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked && cp target/release/papiro /papiro

FROM docker.io/library/alpine:3
# OCR languages baked into the image (space-separated tesseract codes, e.g. "eng deu fra")
ARG OCR_LANGS="spa eng"
# tesseract-ocr-data-osd: orientation detection (fixes pages scanned sideways or upside down)
# tini: tiny PID 1 that reaps orphaned processes, whatever the deployment does
# fonts: without them, PDFs that don't embed their fonts (Helvetica, Times, Arial…) render blank
RUN apk add --no-cache tesseract-ocr tesseract-ocr-data-osd poppler-utils qpdf tzdata tini fontconfig font-liberation font-dejavu \
      $(for l in $OCR_LANGS; do echo tesseract-ocr-data-$l; done) \
    && adduser -D -u 1000 papiro \
    && mkdir -p /data /consume /backup && chown papiro:papiro /data /consume /backup
COPY --from=build /papiro /usr/local/bin/papiro
USER papiro
ENV PAPIRO_DATA=/data PAPIRO_CONSUME=/consume
VOLUME ["/data", "/consume"]
# /backup: mount another volume or disk here and set PAPIRO_BACKUP_DIR=/backup for automatic backups
LABEL org.opencontainers.image.source="https://github.com/elpablazo/papiro" \
      org.opencontainers.image.description="Self-hosted document archive with OCR and instant search. A lightweight Paperless-ngx alternative." \
      org.opencontainers.image.licenses="MIT"
EXPOSE 8000
HEALTHCHECK --interval=30s --timeout=3s CMD wget -qO- http://127.0.0.1:8000/api/health || exit 1
ENTRYPOINT ["/sbin/tini", "--", "papiro"]
