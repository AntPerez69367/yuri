-- Increase password field lengths to accommodate bcrypt hashes (which are 60 characters long).

ALTER TABLE `Character`     MODIFY `ChaPassword` VARCHAR(72) NOT NULL DEFAULT '';
ALTER TABLE `AdminPassword` MODIFY `AdmPassword` VARCHAR(72) NOT NULL DEFAULT '';