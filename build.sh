#! /bin/sh
#
# build.sh
# Copyright (C) 2017 igor <igor@160R>
#
# Distributed under terms of the MIT license.
#


cargo install --force
sudo mv ~/.cargo/bin/page ~/.cargo/bin/page-agent /bin
