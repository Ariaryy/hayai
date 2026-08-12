# Security policy

Hayai is early alpha and currently supports only the latest tagged release.

Please do not open a public issue for vulnerabilities involving launch actions,
elevation, startup registration, shell paths, Scry IPC, clipboard contents, or
native Windows resource handling. Use GitHub's private vulnerability reporting
feature for this repository.

Include the affected Hayai version, Windows version, expected impact, and the
smallest safe reproduction. Do not include private filenames, file-search
results, clipboard data, credentials, or other personal information.

Hayai normally runs unelevated. The bundled Scry setup action is an explicit
boundary that requests elevation to install its per-user daemon task. Preserve
that separation when changing file search or launch behavior.
