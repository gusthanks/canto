# DeviceObject

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**id** | Option<**String**> | The device ID. This ID is unique and persistent to some extent. However, this is not guaranteed and any cached `device_id` should periodically be cleared out and refetched as necessary. | [optional]
**is_active** | Option<**bool**> | If this device is the currently active device. | [optional]
**is_private_session** | Option<**bool**> | If this device is currently in a private session. | [optional]
**is_restricted** | Option<**bool**> | Whether controlling this device is restricted. At present if this is \"true\" then no Web API commands will be accepted by this device. | [optional]
**name** | Option<**String**> | A human-readable name for the device. Some devices have a name that the user can configure (e.g. \\\"Loudest speaker\\\") and some devices have a generic name associated with the manufacturer or device model. | [optional]
**r#type** | Option<**String**> | Device type, such as \"computer\", \"smartphone\" or \"speaker\". | [optional]
**volume_percent** | Option<**i32**> | The current volume in percent. | [optional]
**supports_volume** | Option<**bool**> | If this device can be used to set the volume. | [optional]

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


