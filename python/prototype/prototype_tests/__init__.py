"""The prototypes' tests, which build tables with the core tests' helpers in
``core/tests``, imported as the package ``tests``. They run TableBuilder's
own tests once for each prototype, which Hypothesis's health check of tests
run by several executors would otherwise fail."""

import sys
from pathlib import Path

from hypothesis import HealthCheck, settings

_CORE = str(Path(__file__).parents[2] / "core")
if _CORE not in sys.path:
    sys.path.insert(0, _CORE)

settings.register_profile(
    "prototype", suppress_health_check=[HealthCheck.differing_executors]
)
settings.load_profile("prototype")
