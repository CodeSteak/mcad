#!/usr/bin/env bash
# Python venv with OpenCASCADE (cadquery-ocp, pinned like the test reports) for tests/step/verify.py.
#
#   STEP_VENV=~/venv-step scripts/ci/step-venv.sh     # default STEP_VENV: $HOME/venv-step
#
# Reuses an existing venv that already has the pinned version. Prints STEP_PYTHON=<python>.
set -euo pipefail

OCP_VERSION="${OCP_VERSION:-8.0.1.1.0}"
VENV="${STEP_VENV:-$HOME/venv-step}"
PY="$VENV/bin/python"

have=$("$PY" -m pip show cadquery-ocp 2>/dev/null | sed -n 's/^Version: //p' || true)
if [ "$have" = "$OCP_VERSION" ]; then
  echo "cadquery-ocp $OCP_VERSION: already installed in $VENV"
else
  "${PYTHON:-python3}" -m venv --clear "$VENV"
  "$PY" -m pip install --quiet --upgrade pip
  "$PY" -m pip install --quiet "cadquery-ocp==$OCP_VERSION"
fi
"$PY" -c 'import OCP, sys; print("OCP", OCP.__version__ if hasattr(OCP, "__version__") else "?", "Python", sys.version.split()[0])'
echo "STEP_PYTHON=$PY"
