# makerd önizleme imajı: makerd --mock + web kabuk (compose.yaml /web'e bağlar).
ARG GO_IMAGE=golang:1.26.10-alpine
FROM ${GO_IMAGE} AS build
WORKDIR /src
COPY go.mod go.sum ./
RUN go mod download
COPY . .
ARG VERSION=2.0.0-dev
RUN CGO_ENABLED=0 go build -trimpath -ldflags "-s -w -X main.Version=${VERSION}" -o /out/makerd ./cmd/makerd

FROM alpine:3.24
# tzdata: mock saat dilimleri; busybox sh: web terminal.
RUN apk add --no-cache tzdata \
 && adduser -D -u 1000 -s /bin/sh maker \
 && mkdir -p /web
COPY --from=build /out/makerd /usr/bin/makerd
USER maker
WORKDIR /home/maker
ENV GOMEMLIMIT=48MiB GOGC=50 GOMAXPROCS=2 LANG=C.UTF-8
EXPOSE 8080
ENTRYPOINT ["/usr/bin/makerd"]
CMD ["--mock", "--listen", ":8080", "--web", "/web"]
