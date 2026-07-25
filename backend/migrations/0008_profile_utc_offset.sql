-- Derived from profiles.timezone when the profile is saved. The scorer reads
-- this integer instead of an IANA name so it needs no timezone database and
-- no clock, and so the same pair scores identically all year round.
-- Null for a profile saved before this column existed; it is filled in the
-- next time that profile is written.
ALTER TABLE profiles ADD COLUMN utc_offset_minutes SMALLINT;
