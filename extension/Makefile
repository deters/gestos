UUID := gestos@deters.github.io
PACK := $(UUID).shell-extension.zip

.PHONY: pack install test clean

pack:
	gnome-extensions pack --force --extra-source=lib

install: pack
	gnome-extensions install --force $(PACK)

test:
	gjs -m tests/tile.test.js

clean:
	rm -f $(PACK)
