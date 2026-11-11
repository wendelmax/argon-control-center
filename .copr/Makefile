# Source RPM for Fedora COPR ("make srpm" build method) and for CI.
#   make -f .copr/Makefile srpm outdir=<dir>
# COPR runs it as root in a Fedora chroot from a checkout of the requested tag.
SPEC := packaging/rpm/argon-control-center.spec
SOURCES := $(CURDIR)/.copr/sources
outdir ?= $(CURDIR)/.copr/srpm

srpm:
	command -v rpmbuild >/dev/null && command -v git >/dev/null || dnf -y install git rpm-build
	git config --global --add safe.directory "$(CURDIR)"
	rm -rf "$(SOURCES)" && mkdir -p "$(SOURCES)" "$(outdir)"
	VERSION=$$(rpmspec -q --qf "%{version}\n" $(SPEC) | head -1) && \
	git archive --prefix="argon-control-center-$$VERSION/" \
		-o "$(SOURCES)/argon-control-center-$$VERSION.tar.gz" HEAD
	cp packaging/common/argon-control-center.sysusers "$(SOURCES)/"
	rpmbuild -bs $(SPEC) --define "_sourcedir $(SOURCES)" --define "_srcrpmdir $(outdir)"

.PHONY: srpm
