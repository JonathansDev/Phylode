---------------------------- MODULE DnsSwitch ----------------------------
(* A model of `on` and `off` in src/install/switch.rs: whether `off` always puts every       *)
(* network service's DNS back as it was before the first `on`, whatever order `on`, `off`    *)
(* and an interruption (Ctrl-C, a crash, the Mac losing power) happen in.                   *)
(*                                                                                          *)
(* Each step is one call in the code: a `networksetup` call, reading or writing             *)
(* dns-before-on.tsv. An interruption can fall between any two of them.                     *)
(* Not modelled: a `networksetup` call that fails (the code then keeps the saved file), and  *)
(* someone changing a service's DNS by hand between `on` and `off`.                         *)

EXTENDS TLC

CONSTANTS
    CustomService,  \* a service whose DNS servers were set by hand before `on`
    AutoService,    \* a service on automatic DNS before `on`
    SaveFirst       \* TRUE: the saved file is written before a service is changed

Services == {CustomService, AutoService}
Before == [s \in Services |-> IF s = CustomService THEN "custom" ELSE "auto"]
Gatekeeper == "127.0.0.1"
Nothing == "nothing"
NothingSaved == [s \in Services |-> Nothing]

VARIABLES
    dns,       \* what networksetup holds for each service
    file,      \* DNS_BEFORE_ON_PATH on disk: each service's servers, or Nothing
    saved,     \* the `saved` map inside a running `on`
    pc,        \* "idle", "on" or "off"
    todo,      \* services the running command has yet to visit
    current,   \* the service `on` is between saving and changing, or Nothing
    restored   \* TRUE once an `off` ran to its end, until the next command starts

vars == <<dns, file, saved, pc, todo, current, restored>>

Init ==
    /\ dns = Before
    /\ file = NothingSaved
    /\ saved = NothingSaved
    /\ pc = "idle"
    /\ todo = {}
    /\ current = Nothing
    /\ restored = FALSE

(* on(): read_saved_dns, then the loop over network_services(). *)
StartOn ==
    /\ pc = "idle"
    /\ pc' = "on"
    /\ saved' = file
    /\ todo' = Services
    /\ restored' = FALSE
    /\ UNCHANGED <<dns, file, current>>

(* on(), one service: -getdnsservers, and saved.insert unless it already points at the      *)
(* gatekeeper ("switched on twice"). With SaveFirst, write_saved_dns follows at once.        *)
OnSave ==
    /\ pc = "on" /\ current = Nothing
    /\ \E s \in todo:
        LET kept == IF dns[s] = Gatekeeper THEN saved ELSE [saved EXCEPT ![s] = dns[s]]
        IN /\ saved' = kept
           /\ file' = IF SaveFirst THEN kept ELSE file
           /\ current' = s
           /\ todo' = todo \ {s}
    /\ UNCHANGED <<dns, pc, restored>>

(* on(), one service: -setdnsservers <service> 127.0.0.1. *)
OnSet ==
    /\ pc = "on" /\ current /= Nothing
    /\ dns' = [dns EXCEPT ![current] = Gatekeeper]
    /\ current' = Nothing
    /\ UNCHANGED <<file, saved, pc, todo, restored>>

(* on(), after the loop. Saving last, write_saved_dns came here; saving first, nothing does. *)
FinishOn ==
    /\ pc = "on" /\ todo = {} /\ current = Nothing
    /\ file' = IF SaveFirst THEN file ELSE saved
    /\ saved' = NothingSaved
    /\ pc' = "idle"
    /\ UNCHANGED <<dns, todo, current, restored>>

(* off(): read_saved_dns, then the loop over network_services(). *)
StartOff ==
    /\ pc = "idle"
    /\ pc' = "off"
    /\ todo' = Services
    /\ restored' = FALSE
    /\ UNCHANGED <<dns, file, saved, current>>

(* off(), one service: one pointing at the gatekeeper goes back to what the file saved, or  *)
(* to automatic ("empty") when it saved nothing for it.                                     *)
OffStep ==
    /\ pc = "off"
    /\ \E s \in todo:
        /\ dns' = IF dns[s] = Gatekeeper
                    THEN [dns EXCEPT ![s] = IF file[s] = Nothing THEN "auto" ELSE file[s]]
                    ELSE dns
        /\ todo' = todo \ {s}
    /\ UNCHANGED <<file, saved, pc, current, restored>>

(* off(), after the loop: remove_if_present(saved_path). *)
FinishOff ==
    /\ pc = "off" /\ todo = {}
    /\ file' = NothingSaved
    /\ pc' = "idle"
    /\ restored' = TRUE
    /\ UNCHANGED <<dns, saved, todo, current>>

(* The command stops between two calls: what it held in memory is gone; what networksetup  *)
(* and the disk already hold stays.                                                          *)
Interrupt ==
    /\ pc /= "idle"
    /\ pc' = "idle"
    /\ saved' = NothingSaved
    /\ todo' = {}
    /\ current' = Nothing
    /\ UNCHANGED <<dns, file, restored>>

Next == StartOn \/ OnSave \/ OnSet \/ FinishOn \/ StartOff \/ OffStep \/ FinishOff \/ Interrupt

Spec == Init /\ [][Next]_vars

(* The promise: once `off` has run to its end, every service has what it had before `on`. *)
OffPutsEveryServiceBack == restored => dns = Before
=============================================================================
