# \SearchApi

All URIs are relative to *https://api.spotify.com/v1*

Method | HTTP request | Description
------------- | ------------- | -------------
[**search**](SearchApi.md#search) | **GET** /search | Search for Item 



## search

> models::Search200Response search(q, r#type, market, limit, offset, include_external)
Search for Item 

Get Spotify catalog information about albums, artists, playlists, tracks, shows, episodes or audiobooks that match a keyword string. Audiobooks are only available within the US, UK, Canada, Ireland, New Zealand and Australia markets. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**q** | **String** |  | [required] |
**r#type** | [**Vec<String>**](String.md) |  | [required] |
**market** | Option<**String**> |  |  |
**limit** | Option<**i32**> |  |  |[default to 5]
**offset** | Option<**i32**> |  |  |[default to 0]
**include_external** | Option<**String**> |  |  |

### Return type

[**models::Search200Response**](search_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

