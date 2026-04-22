# Plans And Limits

## Goal

Dropply should feel generous in free mode and premium in paid mode, without making the core product feel cheap or broken.

## Suggested plan model

### Free

- desktop local-first usage
- one account stream when signed in
- limited hosted sync activity
- limited cloud retention/storage
- basic web access

### Pro

- higher sync throughput
- larger file limits
- longer retention
- priority relay capacity
- richer history and recovery
- early access to advanced previews

### Team / Private

- shared/private team streams
- admin controls
- audit policies
- member/device controls
- higher enterprise limits

## Suggested soft limits

These should be enforced server-side for hosted features only.

Free:

- monthly synced item quota
- lower max upload size
- lower concurrent device count
- lower relay bandwidth budget

Pro:

- higher monthly synced item quota
- larger uploads
- more connected devices
- higher relay bandwidth budget

Team:

- pooled limits
- admin overrides
- organization-level rate policy

## Important product rule

Never degrade local core behavior with hosted plan limits.

Limits should apply to:

- hosted sync
- hosted storage
- hosted relay usage
- account-linked web/mobile access

Limits should not apply to:

- local desktop usage
- local item creation
- local export/delete

## UX guidance

- keep free limits simple and visible
- avoid surprise hard failures
- warn before cutoff
- show upgrade prompts only around hosted features

