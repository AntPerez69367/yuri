-- Create KanDonations table for Kan currency payment tracking
-- Referenced in c_src/scripting.c

DROP TABLE IF EXISTS `KanDonations`;

CREATE TABLE `KanDonations` (
  `payment_id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `paypal_account_email` varchar(255) NOT NULL DEFAULT '',
  `game_account_email` varchar(255) NOT NULL DEFAULT '',
  `txn_id` varchar(255) NOT NULL DEFAULT '',
  `payment_status` varchar(50) NOT NULL DEFAULT '',
  `kan_amount` int(10) unsigned NOT NULL DEFAULT '0',
  `Claimed` tinyint(1) unsigned NOT NULL DEFAULT '0',
  `payment_date` datetime DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (`payment_id`),
  KEY `idx_game_email` (`game_account_email`),
  KEY `idx_paypal_email` (`paypal_account_email`),
  KEY `idx_claimed` (`Claimed`),
  KEY `idx_txn_id` (`txn_id`),
  KEY `idx_payment_status` (`payment_status`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE utf8mb4_unicode_ci;
