# GetUsersTopArtistsAndTracks200Response

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**href** | **String** | A link to the Web API endpoint returning the full result of the request  | 
**limit** | **i32** | The maximum number of items in the response (as set in the query or by default).  | 
**next** | Option<**String**> | URL to the next page of items. ( `null` if none)  | 
**offset** | **i32** | The offset of the items returned (as set in the query or by default)  | 
**previous** | Option<**String**> | URL to the previous page of items. ( `null` if none)  | 
**total** | **i32** | The total number of items available to return.  | 
**items** | [**Vec<models::GetUsersTopArtistsAndTracks200ResponseAllOfItemsInner>**](get_users_top_artists_and_tracks_200_response_allOf_items_inner.md) |  | 

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


