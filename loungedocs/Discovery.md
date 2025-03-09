The first part of the cast is how the app discovers other devices. Now, this isn't scoped in how the lounge api works, but it's still the first step to use said api.

## Protocols up the a**

The short answer I found is: [connect-sdk](https://connectsdk.com/en/latest/). Discovering devices is kind of a mess - there are a lot of protocols used by different devices and one shouldn't try to implement them all. Citing the connect-sdk documentation:
 > To communicate with discovered devices, Connect SDK integrates support for protocols such as DLNA, DIAL, SSAP, ECG, AirPlay, Chromecast, UDAP, and webOS second screen protocol

The one I ended up fiddling with the most was the DIAL protocol - basically the second screen device runs a server which may be used to found more information about the device and the apps within it. For some code:

- send a UDP broadcast in the network "M-SEARCH":

```
M-SEARCH * HTTP/1.1
HOST: 239.255.255.250:1900
MAN: "ssdp:discover"
MX: seconds to delay response
ST: urn:dial-multiscreen-org:service:dial:1 USER-AGENT: OS/version product/version
```

- second screen devices in the network will respond something like:

```
HTTP/1.1 200 OK
LOCATION: http://192.168.1.1:52235/dd.xml
CACHE-CONTROL: max-age=1800
EXT:
BOOTID.UPNP.ORG: 1
SERVER: OS/version UPnP/1.1 product/version USN: ​​device UUID
ST: urn:dial-multiscreen-org:service:dial:1 WAKEUP: MAC=10:dd:b1:c9:00:e4;Timeout=10
```

- The important part is the 192.168.x.x:yyyy/dd.xml. By accessing this endpoint, the device will identify itself:

```
<root xmlns="urn:schemas-upnp-org:device-1-0" xmlns:r="urn:restful-tv-org:schemas:upnp-dd">
  <specVersion>
    <major>1</major>
    <minor>0</minor>
  </specVersion>
  <device>
    <deviceType>urn:schemas-upnp-org:device:tvdevice:1</deviceType>
    <friendlyName>PS4-938</friendlyName>
    <manufacturer>Sony</manufacturer>
    <modelName>PS4 Pro</modelName>
    <UDN>uuid:xxxxxxxxxxx</UDN>
  </device>
</root>
```

- [Intermediate steps not fully documented] There appears to be additional steps between getting the device information and querying for app details. This part of the protocol needs further investigation.
- Somewhere in the [dial protocol site](http://www.dial-multiscreen.org), the is a spreadsheet that lists all official DIAL supported apps and identifiers that can be used to query second screen devices with <http://192.168.x.v:zzzz/apps/{app-identifier}>:

```
<service xmlns="urn:dial-multiscreen-org:schemas:dial">
  <name>YouTube</name>
  <options allowStop="false"/>
  <state>running</state>
  <link rel="run" href="http://192.168.x.y:58722//run"/>
  <additionalData>
    <brand>Sony</brand>
    <model>PS4 Pro</model>
    <screenId>qweqweqwe</screenId>
    <theme>??</theme>
    <deviceId>ssssssssss</deviceId>
    <loungeToken>xxxxxxxx</loungeToken>
    <loungeTokenRefreshIntervalMs>1500000</loungeTokenRefreshIntervalMs>
  </additionalData>
</service>
```

- keep the `loungeToken` value in mind, because it will be used extensively in the lounge api
