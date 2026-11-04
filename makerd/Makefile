# makerd: tüm derleme/test Docker içinde (host'ta Go gerekmez).
GO_IMAGE ?= golang:1.26.10-alpine
VERSION  ?= 2.0.0-dev
LDFLAGS  := -s -w -X main.Version=$(VERSION)
DOCKER   := docker run --rm -v "$(CURDIR)":/src -w /src \
	-v makerd-gomod:/go/pkg/mod -v makerd-gocache:/root/.cache/go-build $(GO_IMAGE)

.PHONY: test vet fmt build-arm64 build-amd64 build image clean

test:
	$(DOCKER) sh -c 'go vet ./... && go test -count=1 ./...'

vet:
	$(DOCKER) go vet ./...

fmt:
	$(DOCKER) gofmt -l -w .

build-arm64: ARCH=arm64
build-amd64: ARCH=amd64
build-arm64 build-amd64:
	$(DOCKER) sh -c 'for c in makerd makerd-priv makerd-user; do \
	  CGO_ENABLED=0 GOOS=linux GOARCH=$(ARCH) go build -trimpath -ldflags "$(LDFLAGS)" -o bin/linux-$(ARCH)/$$c ./cmd/$$c || exit 1; done'
	@ls -l bin/linux-$(ARCH)/

build: build-arm64 build-amd64

image:
	docker build --build-arg VERSION=$(VERSION) -t makeros/makerd:dev .

clean:
	rm -rf bin
