---
title: Features
description: Overview of LiveKit Meet features and how they are exposed in the UI.
category: reference
order: 8
nav_order: 8
---

# Features

## End-to-end encryption (E2EE)

E2EE is enabled by default for every room. The per-room passphrase is derived deterministically from `E2EE_SECRET` and the room name, so all participants automatically share the same key. Encrypted rooms show a lock indicator next to participant names.

> Recording is disabled for encrypted rooms because LiveKit cannot record encrypted media.

## Waiting room

Guests joining with `?role=guest` are held in a waiting screen until a host admits them. Hosts see an **Admit** button in the host controls for each waiting guest.

## Breakout rooms

Hosts can create breakout rooms and assign participants. Assigned guests see a banner with the room label and a **Join breakout room** button. Hosts can close all breakout rooms, returning participants to the main room.

## Reactions

Participants can send emoji reactions that float across the screen. The reaction bar is available after joining a room.

## Raise hand

The raise-hand button toggles the participant's hand state. Raised hands appear in a panel visible to all participants, making it easy for hosts to see who wants to speak.

## Recording

Recording is opt-in. When `NEXT_PUBLIC_SHOW_SETTINGS_MENU=true` is set at build time, hosts see a **Recording** tab in the settings panel. Starting a recording creates an egress job and writes the output to the configured S3 bucket.

Settings menu and recording UI can be toggled only at build time.

## Calendar integration

The Bulwark plugin creates meeting links directly from calendar events. The `/meetings` page lists all upcoming meetings for the signed-in user and provides one-click join links.
