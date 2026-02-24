-- Increase password field lengths to accommodate bcrypt hashes (which are 60 characters long).
--
-- DEFAULT '!' is a Unix-style locked-account sentinel: no hashing algorithm (bcrypt, MD5)
-- ever produces '!' as output, so the app can detect uninitialized rows and reject them
-- before attempting a password comparison. A row with '!' must receive a valid hash via
-- the bcrypt rehash-on-login path or a manual backfill before the account can be used.

ALTER TABLE `Character`     MODIFY `ChaPassword` VARCHAR(72) NOT NULL DEFAULT '!';
ALTER TABLE `AdminPassword` MODIFY `AdmPassword` VARCHAR(72) NOT NULL DEFAULT '!';