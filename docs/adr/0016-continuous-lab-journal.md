# Continuous Lab Journal

The primary interactive evidence source is one append-only Lab Journal opened when a Lab starts and finalized when it stops. Ronchi records every event included by the active Observation Boundary and persistence policy; a fault is a marked event in that stream. Investigations, Timelines, Findings, and token-efficient Evidence Slices are filtered views over the Journal rather than separately started captures. This avoids losing unknown earlier context in a long-running backend while keeping CI-style Runs optional.
