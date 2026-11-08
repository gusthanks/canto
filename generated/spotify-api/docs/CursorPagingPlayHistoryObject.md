# CursorPagingPlayHistoryObject

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**href** | Option<**String**> | A link to the Web API endpoint returning the full result of the request. | [optional]
**limit** | Option<**i32**> | The maximum number of items in the response (as set in the query or by default). | [optional]
**next** | Option<**String**> | URL to the next page of items. ( `null` if none) | [optional]
**cursors** | Option<[**models::CursorObject**](CursorObject.md)> | The cursors used to find the next set of items. | [optional]
**total** | Option<**i32**> | The total number of items available to return. | [optional]
**items** | Option<[**Vec<models::PlayHistoryObject>**](PlayHistoryObject.md)> |  | [optional]

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


