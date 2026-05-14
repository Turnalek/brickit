#!/bin/bash
#

set -e

# create egress from host tun interface
sudo ip tuntap add host_egress mode tun

# assign 10.0.0.2/28 to host_egress to mask the martians
sudo ip address add 10.0.0.2/28 dev host_egress

# bring the interface up
sudo ip link set host_egress up

# ensure forwarding is going to go through
# echo 1 > /proc/sys/net/ipv4/ip_forward # should be set already
sudo iptables -P FORWARD ACCEPT

# masquerade nat for egress
sudo iptables -t nat -I POSTROUTING -s 10.0.0.1 -j MASQUERADE -o "wlan0"

# check it
ip a show dev host_egress

# DEBUG STUFF
#
# sudo tcpdump -i host_egress -vv -n host 109.123.250.238
# 
