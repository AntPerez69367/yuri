-- ----------------------------
-- Allow accounts to be created without passwords
-- ----------------------------
ALTER TABLE `Accounts` MODIFY `AccountPassword` varchar(255) NOT NULL DEFAULT '';
