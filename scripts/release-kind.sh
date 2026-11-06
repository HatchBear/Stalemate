#!/usr/bin/env bash
# Classifies a release tag. Prints alpha, beta, stable or invalid (exit 1 for invalid).
#   26w45a, 26w45a2   week-name closed Alpha (DEC-077)
#   v*-alpha.*        closed Alpha (DEC-071)
#   v0.*              beta (0.x prerelease)
#   other v*          stable
set -u
tag="${1:-}"
if [[ "$tag" =~ ^[0-9]{2}w[0-9]{2}a([0-9]+)?$ ]]; then
  echo alpha
elif [[ "$tag" == v*-alpha.* ]]; then
  echo alpha
elif [[ "$tag" == v0.* ]]; then
  echo beta
elif [[ "$tag" == v* ]]; then
  echo stable
else
  echo invalid
  exit 1
fi
