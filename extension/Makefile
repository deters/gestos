UUID := gestos@deters.github.io
PACK := $(UUID).shell-extension.zip
# The files packed, gathered here so that the metadata can carry the code's fingerprint.
STAGE := .stage

.PHONY: pack install test clean

pack:
	rm -rf $(STAGE)
	mkdir -p $(STAGE)
	cp -r *.js lib schemas $(STAGE)/
	sh fingerprint.sh $(STAGE)/metadata.json
	gnome-extensions pack --force --extra-source=lib $(STAGE)

install: pack
	gnome-extensions install --force $(PACK)

test:
	gjs -m tests/tile.test.js

clean:
	rm -rf $(PACK) $(STAGE)
