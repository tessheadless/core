#!/usr/bin/env bash

git push;
git checkout main;
git push;
git push --tags;
gh release create "$1"
