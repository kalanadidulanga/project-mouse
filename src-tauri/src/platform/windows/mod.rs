//! Windows implementations of the platform traits. The only real backend in M1.

pub mod power;
// ponytail: autostart (HKCU\Run) arrives in Phase 4 with the Win32_System_Registry feature.
